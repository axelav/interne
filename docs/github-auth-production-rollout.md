# GitHub authentication production rollout

The GitHub-auth implementation is complete in this repository. This runbook covers
the operational work that remains before it can serve production traffic at
`https://interne.honkytonk.in`.

Keep `GITHUB_SIGNUP_MODE=closed` throughout the rollout. Do not switch to public
signup until the existing account has been connected and the production smoke test
passes.

## Completion checklist

- [ ] Prove the existing production invite code still opens the intended account.
- [ ] Register the production GitHub OAuth app.
- [ ] Add the OAuth and auth settings to `honkytonk-infra` without committing the
      client secret.
- [ ] Repair and test the ongoing `interne` backup job so it cannot archive the
      wrong Compose volume.
- [ ] Inspect query-string logging configuration before deployment, then prove the
      live stack omits query strings after deployment.
- [ ] Take and verify a restorable SQLite backup.
- [ ] Record both repository revisions and preserve the prior application image.
- [ ] Deploy in closed mode and confirm the schema migration succeeds.
- [ ] Connect the existing `axelav` account with its legacy invite code.
- [ ] Complete a real GitHub sign-out/sign-in smoke test against the existing data.
- [ ] Confirm the recovery command is available without issuing a production reset.

## 0. Verify the legacy migration credential

Before changing configuration or stopping the old application, open the current
production `/login` page in a private browser session and use the exact invite code
intended for the existing account. Confirm that it opens the expected account and
existing data, then close that private session. Do not paste the invite code into a
shell command, log, issue, or rollout record.

The new release intentionally treats every pre-migration session as logged out
because those sessions have no auth-version value. The verified invite code is
therefore required once after deployment to connect the existing account to
GitHub.

Gate: the operator possesses the invite code and has just proved, against the old
production release, that it opens the intended account.

## 1. Register the GitHub OAuth app

Create one production OAuth app under the GitHub account or organization that will
own the integration.

Use exactly:

- Application name: `Interne` (or another unambiguous production name)
- Homepage URL: `https://interne.honkytonk.in`
- Authorization callback URL:
  `https://interne.honkytonk.in/auth/github/callback`
- Scopes: none; do not request a scope

Record the generated client ID and client secret in the deployment's secret
store. The client secret must not be committed to either `interne` or
`honkytonk-infra`, pasted into logs, or included in review output.

Gate: the OAuth app shows the exact callback above, and both credentials are
available to the production deploy without appearing in Git.

## 2. Configure `honkytonk-infra`

The sibling repository currently gives `interne` only `DATABASE_URL` and
`RUST_LOG`. Extend the `interne` service in
`honkytonk-infra/docker-compose.yml` with:

```yaml
environment:
  - DATABASE_URL=sqlite:/app/data/interne.db
  - RUST_LOG=info
  - GITHUB_CLIENT_ID=${GITHUB_CLIENT_ID:?Set GITHUB_CLIENT_ID}
  - GITHUB_CLIENT_SECRET=${GITHUB_CLIENT_SECRET:?Set GITHUB_CLIENT_SECRET}
  - PUBLIC_BASE_URL=https://interne.honkytonk.in
  - GITHUB_SIGNUP_MODE=closed
  - SECURE_COOKIES=true
```

Provide the two GitHub credentials through the server's existing untracked secret
environment. If that environment is the repository's `.env` file, restrict it to
the deployment owner before adding the secret:

```bash
chmod 0600 .env
stat -c '%a %U %G %n' .env
```

Keep `SECURE_COOKIES=true` in production. Do not print the rendered Compose model,
container environment, or `.env` contents during validation or review.

Before deployment, validate the Compose model without printing the interpolated
secret:

```bash
docker compose config --quiet
```

Gate: the secret source is owner-readable only, Compose renders successfully when
it is present, and Compose fails when either GitHub credential is absent.

## 3. Inspect query-string logging

Interne records only the request path in its own tracing spans. The production
proxy is Traefik, and its current Compose configuration does not explicitly enable
access logs. Verify the live configuration anyway, including any CDN, host-level
web server, metrics collector, or log shipper in front of Traefik.

Before deployment, confirm that no layer is configured to record the raw request
target or URI with its query string. The end-to-end canary in step 6 verifies the
new application's path-only tracing together with the live proxy stack.

Gate: every request-logging layer is identified and configured to omit query
strings.

## 4. Repair backups and take the rollout backup

Take an offline archive immediately before deploying the migration. Stopping only
`interne` provides a consistent copy while leaving Traefik and unrelated services
running.

The existing `honkytonk-infra/scripts/backup-interne.sh` refers to a volume named
`interne-data`; Compose prefixes that name unless the volume has an explicit
`name`. The script can therefore create and successfully archive an empty volume.
Repair it in `honkytonk-infra` before rollout. The repaired job must use the
`interne` container's `/app/data` mount (or the volume name resolved from Compose),
stop `interne` or use SQLite's online backup API, guarantee restart after both
success and failure with a trap/finally path when it stops the service, verify that
the archive contains a non-empty `interne.db`, and only then apply retention.

For the rollout backup, choose a new timestamped path, fail if it already exists,
and record that exact path in the rollout log and rollback commands. Use the
stopped container's mounted volume directly:

```bash
docker compose stop interne
install -d -m 0700 /home/deploy/backups/interne
ROLLOUT_BACKUP_FILENAME="interne-before-github-auth-$(date -u +%Y%m%d-%H%M%S).tar.gz"
ROLLOUT_BACKUP="/home/deploy/backups/interne/$ROLLOUT_BACKUP_FILENAME"
test ! -e "$ROLLOUT_BACKUP"
docker run --rm \
  --volumes-from interne \
  -v /home/deploy/backups/interne:/backup \
  -e BACKUP_FILENAME="$ROLLOUT_BACKUP_FILENAME" \
  alpine sh -c \
  'umask 077; tar czf "/backup/$BACKUP_FILENAME" -C /app/data .'
chmod 0600 "$ROLLOUT_BACKUP"
stat -c '%a %U %G %n' "$ROLLOUT_BACKUP"
tar tzf "$ROLLOUT_BACKUP"
```

Never reuse a path from an earlier attempt. Keep the recorded filename available
for the rollback procedure.

Restore the archive into a new verification directory and run SQLite's integrity
check against the extracted copy. The temporary Alpine container may download the
`sqlite` package if it is not already cached:

```bash
mkdir -m 0700 /home/deploy/backups/interne/restore-check-github-auth
tar xzf "$ROLLOUT_BACKUP" \
  -C /home/deploy/backups/interne/restore-check-github-auth
test -s /home/deploy/backups/interne/restore-check-github-auth/interne.db
docker run --rm \
  -v /home/deploy/backups/interne/restore-check-github-auth:/data:ro \
  alpine:3.21 sh -c \
  'apk add --no-cache sqlite >/dev/null && sqlite3 /data/interne.db "PRAGMA integrity_check;"'
```

The final command must print exactly `ok`. Do not continue if the archive cannot
be extracted, the database is empty, or the integrity check fails. Keep the
application stopped until the deploy begins, or restart the old image if the
deploy is postponed.

Gate: the repaired ongoing backup job has passed a restore test, and the rollout
archive is owner-readable only and extracts to a non-empty database whose
`PRAGMA integrity_check` is `ok`.

## 5. Pin artifacts and deploy in closed mode

Record the exact source revisions and require clean worktrees. Compare the Interne
revision to the approved GitHub-auth merge commit before continuing:

```bash
git rev-parse HEAD
git status --short
git -C ../interne rev-parse HEAD
git -C ../interne status --short
```

Both status commands must be blank. Record both printed commit IDs in the rollout
log. Before rebuilding, record the current container's image name and ID, then
preserve the image under a rollback tag:

```bash
docker inspect interne --format 'image-name={{.Config.Image}} image-id={{.Image}}'
docker image tag "$(docker inspect interne --format '{{.Image}}')" \
  interne:pre-github-auth
```

Record the `image-name` value because it is required by the rollback procedure.
Then rebuild only the `interne` service from the verified source revision:

```bash
docker compose up -d --build interne
docker compose ps interne
docker compose logs --tail=100 interne
curl -fsS -o /dev/null https://interne.honkytonk.in/login
```

The process must start without configuration or migration errors. If startup
fails, stop the new container, restore the pre-deploy archive into the same volume,
and redeploy the prior application revision before investigating further.
Expect every existing browser session to be logged out at this point; that is the
intended auth-version migration behavior, not a failed deploy.

Gate: the container is running, `/login` responds over HTTPS, and startup logs show
no migration or OAuth configuration failure.

## 6. Prove query strings are not logged

After the new container is live, use a fake canary, never an invitation or recovery
token. Generate a new canary and record the start time for every attempt:

```bash
LOG_CHECK_SINCE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
LOG_CANARY="PROXY_LOG_CANARY_$(openssl rand -hex 12)"
curl -sS -o /dev/null \
  "https://interne.honkytonk.in/recover?token=${LOG_CANARY}"
docker compose logs --since "$LOG_CHECK_SINCE" interne traefik | grep "$LOG_CANARY"
```

Also search every upstream log destination identified in step 3 if logs leave
Docker, bounding that search to `LOG_CHECK_SINCE`. The request may appear as
`/recover`, but the canary value must not appear anywhere. `grep` should exit 1
with no output. If the canary appears, stop the rollout, configure that layer to
omit query strings, generate a different canary and timestamp, and repeat the
check.

Gate: no application, proxy, platform, or shipped log contains the canary value.

## 7. Run the real-provider browser smoke test

Use a normal browser so the test exercises production cookies, redirects, Traefik,
GitHub, and the real callback together.

1. Open `/login` and use the existing invite code for the production account.
2. Choose **Connect GitHub**, authorize the production OAuth app as `axelav`, and
   confirm the displayed GitHub username.
3. Verify the existing entries, visits, collections, and tags are still present.
4. Log out.
5. Choose **Sign in with GitHub** and verify the same account and existing data
   open again.
6. In a private browser session, start a sign-in and cancel at GitHub. Confirm the
   app returns a safe, actionable error and no credential appears in the URL or
   page.

The first successful connection consumes the legacy invite code and increments the
account's auth version. Do not repeat the migration flow after it succeeds.

Gate: GitHub login reaches the original account and data after a full logout, with
secure cookies and no sensitive values in browser-visible errors or logs.

## 8. Confirm recovery readiness

Verify that the deployed binary exposes the recovery command without executing it
for the production user:

```bash
docker compose exec interne interne help
```

The output must list `reset-auth`. Do not run `reset-auth <user-id>` as a smoke
test: it revokes all sessions and creates a credential that can replace the linked
GitHub identity.

Gate: the operator knows where to find the user ID, how to run `reset-auth` over
SSH, and how to transmit its four-hour, single-use URL privately if recovery is
ever required.

## Rollback procedure

Use this only with the verified archive, the `interne:pre-github-auth` image, and
the prior Compose image name recorded in step 5. Confirm the stopped container's
mount destination before changing any data:

```bash
docker compose stop interne
docker inspect interne --format \
  '{{range .Mounts}}{{println .Destination .Name .Source}}{{end}}'
```

The output must identify the expected production volume at `/app/data`. Stop if it
does not. Remove only Interne's SQLite files, extract the verified archive through
the same container mount, and re-run the integrity check:

```bash
docker run --rm --volumes-from interne alpine:3.21 sh -c \
  'rm -f /app/data/interne.db /app/data/interne.db-wal /app/data/interne.db-shm /app/data/interne.db-journal'
docker run --rm \
  --volumes-from interne \
  -v /home/deploy/backups/interne:/backup:ro \
  alpine:3.21 tar xzf /backup/<recorded-rollout-backup-filename> -C /app/data
docker run --rm --volumes-from interne alpine:3.21 sh -c \
  'apk add --no-cache sqlite >/dev/null && sqlite3 /app/data/interne.db "PRAGMA integrity_check;"'
```

After the integrity check prints `ok`, retag the preserved image with the exact
Compose image name recorded in step 5, then recreate without building. Replace the
placeholder; do not type the angle brackets:

```bash
docker image tag interne:pre-github-auth <recorded-compose-image-name>
docker compose up -d --no-build --force-recreate interne
docker compose logs --tail=100 interne
```

Gate: the previous image is running against the restored database, startup logs
are clean, and `/login` responds over HTTPS.

## Rollback boundary

Before the existing account is connected, rollback is the prior application image
plus the verified pre-deploy database archive. After GitHub is connected and the
legacy invite is consumed, prefer fixing forward. Restoring the old archive after
new writes would discard those writes and re-enable the old invite credential.

## After rollout

Keep signup closed unless public registration is deliberately approved. Once every
legacy user is connected and the migration window has ended, follow the deferred
cleanup in the design and implementation plan to remove invite-code login and its
deprecated compatibility surface. Invitation and recovery links remain supported.
