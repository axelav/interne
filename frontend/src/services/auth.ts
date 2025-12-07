import type {
  LoginCredentials,
  RegisterCredentials,
  User,
} from "../types/user";
import { trailbase } from "./trailbase";

export async function login(
  credentials: LoginCredentials,
): Promise<User> {
  const formData = new URLSearchParams();
  formData.append("email", credentials.email);
  formData.append("password", credentials.password);

  // TrailBase returns a redirect (303), but sets auth cookies
  // We submit the form and let the browser handle cookies
  await fetch("/api/auth/v1/login", {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
    },
    body: formData.toString(),
    credentials: 'include',
  });

  // Check if we're now authenticated by fetching current user
  const user = await getCurrentUser();
  if (!user) {
    throw new Error("Login failed - invalid credentials");
  }
  return user;
}

export async function register(
  credentials: RegisterCredentials,
): Promise<User> {
  const formData = new URLSearchParams();
  formData.append("email", credentials.email);
  formData.append("password", credentials.password);
  formData.append("password_repeat", credentials.password_repeat);

  // TrailBase returns a redirect (303), but sets auth cookies
  await fetch("/api/auth/v1/register", {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
    },
    body: formData.toString(),
    credentials: 'include',
  });

  // Check if we're now authenticated
  const user = await getCurrentUser();
  if (!user) {
    throw new Error("Registration failed");
  }
  return user;
}

export async function logout(): Promise<void> {
  await trailbase.request<void>("/auth/v1/logout", {
    method: "POST",
  });
}

export async function getCurrentUser(): Promise<User | null> {
  try {
    const response = await trailbase.request<any>("/auth/v1/status");
    // TrailBase returns {auth_token: "...", ...} when authenticated
    // or {auth_token: null, ...} when not authenticated
    if (response && response.auth_token) {
      // Decode JWT to get user info
      const payload = JSON.parse(atob(response.auth_token.split('.')[1]));
      return {
        id: payload.sub,
        email: payload.email,
        verified: true, // If we have a token, user is verified
        admin: false, // We don't have this info in the token
        created: payload.iat,
      };
    }
    return null;
  } catch {
    return null;
  }
}
