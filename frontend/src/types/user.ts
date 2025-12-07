export interface User {
  id: string;
  email: string;
  verified: boolean;
  admin: boolean;
  created: number;
}

export interface AuthResponse {
  auth_token: string;
  refresh_token: string;
  csrf_token: string;
}

export interface LoginCredentials {
  email: string;
  password: string;
}

export interface RegisterCredentials {
  email: string;
  password: string;
  password_repeat: string;
}
