# GitHub Authentication Design

## Summary

Interne will replace routine invite-code login with GitHub OAuth. The first release will run in closed signup mode: existing users can connect a GitHub identity, and an administrator can create invitation links for specific new users. A later configuration change can enable public signup for anyone with a GitHub account.

Interne will not send email, request a GitHub email scope, or store GitHub access tokens. Emergency recovery remains an explicit SSH-only administrative action that produces a four-hour, single-use recovery URL.

## Goals

- Let an existing user sign in from a new browser through GitHub without SSH access.
- Preserve the existing user's ID, entries, visits, collections, and tags when GitHub is connected.
- Launch closed, while making public GitHub signup a configuration-only change.
- Support manually invited users without operating an email service.
- Provide a safe emergency recovery path.
- Make the legacy `invite_code` column removable after migration.

## Non-goals

- Password authentication.
- Email-based login, verification, invitations, or recovery.
- A general account-settings or user-administration UI.
- Persisting GitHub API access tokens.
- Self-service GitHub disconnection.
- Account merging.
- Multiple GitHub identities per Interne user.

## Chosen approach

Interne will implement the GitHub OAuth web application flow directly. This keeps account linking and signup policy in the application that owns the user records. A reverse-proxy authentication service would still require identity-header trust and account-linking logic, while adding another deployed service. Passkeys would require a more complex enrollment and recovery design.

GitHub's stable numeric user ID is the external identity. The GitHub username is display and diagnostic metadata only; changing a GitHub username must not break login.

## Configuration

The server reads:

- `GITHUB_CLIENT_ID`: OAuth application client ID.
- `GITHUB_CLIENT_SECRET`: OAuth application client secret.
- `PUBLIC_BASE_URL`: canonical external origin. Production uses `https://interne.honkytonk.in`.
- `GITHUB_SIGNUP_MODE`: `closed` or `public`; defaults to `closed`.

Server startup fails with an actionable error if the OAuth credentials or public base URL are missing or invalid. CLI commands that generate URLs require `PUBLIC_BASE_URL` but do not require GitHub credentials.

The production GitHub OAuth application uses:

- Homepage: `https://interne.honkytonk.in`
- Callback: `https://interne.honkytonk.in/auth/github/callback`
- OAuth scopes: none

## Data model

### Users

The `users` table gains:

- `github_user_id TEXT UNIQUE NULL`: authoritative external identity, stored as an opaque decimal string.
- `github_login TEXT NULL`: most recently observed GitHub username.
- `auth_version INTEGER NOT NULL DEFAULT 1`: session-revocation generation.

The existing `invite_code` becomes nullable. Setting it to `NULL` permanently disables legacy invite-code authentication for that user. A later migration may remove the column and legacy route without changing GitHub login or recovery.

GitHub profile email is neither requested nor stored. A new public user's Interne name is GitHub's non-empty display name, falling back to the GitHub username.

### Connection tokens

A new `auth_connection_tokens` table contains:

- token ID;
- target Interne user ID;
- SHA-256 hash of the random bearer token, uniquely indexed;
- purpose: `invite` or `recovery`;
- creation and expiration timestamps;
- nullable consumption timestamp.

The plaintext token appears only in the generated URL printed by the CLI. It is never stored in SQLite. Tokens expire after four hours and are single-use. Issuing a token invalidates every still-active connection token for the same target user.

An invitation token and a recovery token use the same browser flow. Their different purposes exist for auditability and user-facing copy.

## Module boundaries

GitHub-specific HTTP behavior lives behind a narrow OAuth client interface. The rest of the application can:

1. construct an authorization URL;
2. exchange a callback code using a PKCE verifier;
3. fetch the authenticated GitHub profile.

The route layer owns Interne flow state, account lookup/linking, session creation, signup policy, confirmation pages, and error responses. Database operations for consuming a connection token and linking an identity occur transactionally.

The access token returned by GitHub exists only in memory while fetching `/user` and is then dropped.

## OAuth security

Every OAuth attempt uses:

- a cryptographically random `state` value to prevent callback CSRF;
- PKCE with `S256` and a fresh verifier;
- an explicit callback URL derived from `PUBLIC_BASE_URL`;
- a server-side session record identifying the attempt's purpose.

Only one OAuth attempt is active per browser session. Starting another replaces the prior attempt. The callback must match the stored state and have an unexpired flow record. Interne always fetches `/user` after exchanging a code; it never trusts identity values from browser input.

OAuth attempts and pending confirmations expire after ten minutes. Restricted legacy-migration sessions expire after thirty minutes. These deadlines are stored with the flow state and checked independently of the session cookie's longer inactivity lifetime.

For link and recovery flows, the callback stores only the fetched GitHub ID, login, and display name as a short-lived pending connection in the server-side session. An explicit confirmation POST completes the link. The confirmation page identifies the GitHub username being connected. Normal login and public signup do not require a second confirmation.

If the returned GitHub ID is already connected to another Interne user, the operation fails without modifying either account. Interne never matches accounts by GitHub username, display name, or email.

## User flows

### Normal GitHub login

The login page's primary action is **Continue with GitHub**.

1. Interne starts a `login` OAuth attempt.
2. GitHub redirects to the callback.
3. Interne looks up `github_user_id`.
4. If found, Interne updates `github_login`, cycles the session ID, stores the user ID and current `auth_version`, and redirects home.
5. If not found and signup mode is `closed`, Interne shows: “Access isn’t open yet. Email webmaster@honkytonk.in for an invite.”
6. If not found and signup mode is `public`, Interne creates and logs in a user immediately.

Changing `GITHUB_SIGNUP_MODE` from `closed` to `public` and restarting the container is sufficient to open registration. No code or schema deployment is required.

### Existing-user migration

The legacy invite-code form remains temporarily available for users whose `invite_code` is non-null and whose GitHub identity is unconnected.

1. A valid invite code creates a restricted migration session and redirects to the GitHub connection page.
2. That session may only connect GitHub or log out; it cannot access normal application data routes.
3. Interne completes a `link` OAuth attempt and shows “Connect Interne to `<github_login>`?”
4. Confirmation transactionally sets `github_user_id` and `github_login`, sets `invite_code` to `NULL`, consumes any active connection tokens, increments `auth_version`, and creates a fresh full session.

Sessions created before the auth-version migration do not contain a version and are treated as logged out. The production rollout therefore requires the existing invite code once. This is the last routine use of that credential.

The legacy form is rendered only while at least one unlinked user retains a legacy invite code. Once all such codes are gone, only GitHub login is shown. A later cleanup removes the route, model field, column, and deprecated command.

### Closed-mode invitation

`interne invite-user <name>`:

1. creates an unlinked Interne user with no legacy invite code;
2. invalidates any prior active connection token for that new user (normally none);
3. issues a four-hour invitation token;
4. prints the new user ID and complete connection URL.

The administrator sends the URL manually. Opening it validates the token, establishes a short-lived recovery/linking session, and immediately redirects to a clean URL before starting GitHub OAuth. After the callback, the recipient confirms the GitHub username. Confirmation links the identity, consumes the token, cycles the session ID, and logs the user in.

The legacy `create-user` command remains as a deprecated compatibility alias during the transition. It follows the invitation behavior rather than generating a new long-lived invite code. It can be removed with the legacy column.

### Emergency recovery

`interne reset-auth <user-id>` is deliberately available only through CLI access to the deployment:

1. verify the user exists;
2. clear `github_user_id` and `github_login`;
3. set `invite_code` to `NULL`;
4. increment `auth_version`, immediately invalidating every existing session;
5. invalidate earlier connection tokens;
6. create a four-hour, single-use recovery token;
7. print the complete recovery URL.

The recovery URL allows any GitHub identity not already attached elsewhere to be connected to the target user after explicit confirmation. Possession of the URL is therefore equivalent to recovery authority. There is no self-service unlink action.

### Recovery URL handling

The CLI prints a URL shaped like `https://interne.honkytonk.in/recover?token=<plaintext-token>`. The recovery/invitation endpoint validates the plaintext token by hashing it and comparing the hash to an active, unexpired database record. It then records only the token ID and purpose in the server-side session and redirects immediately to a URL without the token before OAuth begins. Responses set a restrictive referrer policy, and request logging for this endpoint must omit the query string.

Expired, consumed, malformed, and superseded tokens all produce the same safe, actionable failure page. Confirmation rechecks the stored token record's target, expiration, and consumption state, then consumes the token and links the identity in one transaction, preventing replay and partial updates.

## Sessions and authorization

A full authenticated session stores both the Interne user ID and the `auth_version` observed at login. `AuthUser` accepts the session only when both values match the current user row. This keeps all existing protected route interfaces unchanged while enabling immediate global logout through a version increment.

OAuth attempts, restricted migration sessions, and pending confirmations are separate session states and cannot satisfy `AuthUser`. Successful login or linking cycles the session ID. Logout continues to flush the session.

The existing 30-day inactivity expiry remains unchanged. A temporary GitHub outage does not end an existing valid Interne session, but a new login requires GitHub to be available.

## Error handling

User-facing errors provide a next action without exposing OAuth codes, access tokens, recovery tokens, client secrets, database details, or raw GitHub responses. Cases include:

- denied or failed GitHub authorization;
- mismatched or missing OAuth state;
- expired OAuth attempt;
- GitHub token exchange or profile lookup failure;
- GitHub identity already connected elsewhere;
- closed signup for an unknown GitHub identity;
- expired, used, malformed, or superseded connection link;
- invalid signup-mode or URL configuration.

Provider and database failures are logged with internal context, with credentials and bearer values redacted.

## Testing

The GitHub client is replaceable in tests so integration tests never call GitHub. Tests cover:

- authorization URLs include state, PKCE, callback, and no requested scopes;
- callback state and PKCE validation;
- login to an existing linked account;
- refresh of display-only `github_login`;
- closed-mode rejection with the webmaster contact;
- public-mode creation and name fallback;
- legacy login's restricted session and successful connection;
- legacy invite invalidation after connection;
- invitation and recovery confirmation;
- duplicate GitHub identity rejection;
- expired, consumed, malformed, and superseded connection tokens;
- transactional token consumption under replay attempts;
- `reset-auth` session revocation through `auth_version`;
- logout and existing protected-route behavior;
- migration preservation of existing users, entries, visits, collections, and tags.

CLI tests verify generated URLs, four-hour expiration, token hashing, invalid user handling, and the deprecated alias.

## Rollout

1. Register the production GitHub OAuth application with the configured homepage and callback.
2. Add the OAuth credentials, `PUBLIC_BASE_URL`, and `GITHUB_SIGNUP_MODE=closed` to deployment configuration.
3. Back up the SQLite database.
4. Deploy the application and schema migration once.
5. Sign in with the existing invite code and connect GitHub account `axelav`.
6. Log out and verify GitHub login opens the same existing entries.
7. Verify `reset-auth` can generate a recovery URL, without executing a reset on the production user.
8. Keep closed mode until public signup is desired; then change only `GITHUB_SIGNUP_MODE=public` and restart.

## Legacy cleanup

After the production account is connected and the migration period has ended, a follow-up cleanup may:

- remove invite-code request handling and its conditional login form;
- remove the deprecated `create-user` compatibility alias;
- remove `invite_code` from the Rust user model;
- rebuild the SQLite `users` table without the `invite_code` column.

Invitation and recovery continue through `auth_connection_tokens`, so this cleanup does not alter the supported authentication model.

## References

- [GitHub: Authorizing OAuth apps](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [GitHub: Creating an OAuth app](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/creating-an-oauth-app)

## Future Work

- [ ] Remove the legacy invite-code route, UI, model field, column, and deprecated `create-user` alias after all existing users have connected GitHub.
