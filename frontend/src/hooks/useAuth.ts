import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import * as authService from "../services/auth";
import type { LoginCredentials, RegisterCredentials, User } from "../types/user";

export function useCurrentUser() {
  const [user, setUser] = useState<User | null>(authService.getCurrentUser);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    // Initial check is synchronous with PocketBase authStore
    setUser(authService.getCurrentUser());
    setIsLoading(false);

    // Subscribe to auth changes
    const unsubscribe = authService.onAuthChange((newUser) => {
      setUser(newUser);
    });

    return unsubscribe;
  }, []);

  return {
    data: user,
    isLoading,
    isError: false,
    error: null,
  };
}

export function useLogin() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (credentials: LoginCredentials) =>
      authService.login(credentials),
    onSuccess: () => {
      // Invalidate entries query since they're user-specific
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    },
  });
}

export function useRegister() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (credentials: RegisterCredentials) =>
      authService.register(credentials),
    onSuccess: () => {
      // Invalidate entries query since they're user-specific
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    },
  });
}

export function useLogout() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: authService.logout,
    onSuccess: () => {
      // Clear all cached data on logout
      queryClient.clear();
    },
  });
}
