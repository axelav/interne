import type { Entry } from '../types/entry'
import type { AuthResponse, LoginCredentials, RegisterCredentials } from '../types/user'

const API_BASE = '/api'

class TrailBaseClient {
  private accessToken: string | null = null

  async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      ...options.headers,
    }

    if (this.accessToken) {
      headers['Authorization'] = `Bearer ${this.accessToken}`
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers,
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: response.statusText }))
      throw new Error(error.message || 'Request failed')
    }

    return response.json()
  }

  setAccessToken(token: string) {
    this.accessToken = token
    localStorage.setItem('access_token', token)
  }

  clearAccessToken() {
    this.accessToken = null
    localStorage.removeItem('access_token')
  }

  getAccessToken(): string | null {
    if (!this.accessToken) {
      this.accessToken = localStorage.getItem('access_token')
    }
    return this.accessToken
  }
}

export const trailbase = new TrailBaseClient()
