import type {
  AuthResponse,
  LoginCredentials,
  RegisterCredentials,
  User,
} from "../types/user";
import { trailbase } from "./trailbase";

export async function login(
  credentials: LoginCredentials,
): Promise<AuthResponse> {
  const response = await trailbase.request<AuthResponse>("/auth/login", {
    method: "POST",
    body: JSON.stringify(credentials),
  });

  trailbase.setAccessToken(response.access_token);
  return response;
}

export async function register(
  credentials: RegisterCredentials,
): Promise<AuthResponse> {
  const response = await trailbase.request<AuthResponse>("/auth/register", {
    method: "POST",
    body: JSON.stringify(credentials),
  });

  trailbase.setAccessToken(response.access_token);
  return response;
}

export async function logout(): Promise<void> {
  trailbase.clearAccessToken();
}

export async function getCurrentUser(): Promise<User | null> {
  const token = trailbase.getAccessToken();
  if (!token) return null;

  try {
    return await trailbase.request<User>("/auth/me");
  } catch {
    trailbase.clearAccessToken();
    return null;
  }
}
