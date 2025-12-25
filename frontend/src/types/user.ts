import type { RecordModel } from 'pocketbase';

export interface User extends RecordModel {
  email: string;
  verified: boolean;
  name?: string;
}

export interface LoginCredentials {
  email: string;
  password: string;
}

export interface RegisterCredentials {
  email: string;
  password: string;
  passwordConfirm: string;
}
