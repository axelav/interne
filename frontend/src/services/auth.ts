import type { LoginCredentials, RegisterCredentials, User } from "../types/user";
import { pb } from "./pocketbase";

export async function login(credentials: LoginCredentials): Promise<User> {
  const authData = await pb.collection('users').authWithPassword(
    credentials.email,
    credentials.password
  );
  return authData.record as User;
}

export async function register(credentials: RegisterCredentials): Promise<User> {
  await pb.collection('users').create({
    email: credentials.email,
    password: credentials.password,
    passwordConfirm: credentials.passwordConfirm,
  });
  // Auto-login after registration
  return login({ email: credentials.email, password: credentials.password });
}

export async function logout(): Promise<void> {
  pb.authStore.clear();
}

export function getCurrentUser(): User | null {
  if (!pb.authStore.isValid) {
    return null;
  }
  return pb.authStore.model as User | null;
}

export function onAuthChange(callback: (user: User | null) => void): () => void {
  return pb.authStore.onChange(() => {
    callback(getCurrentUser());
  });
}
