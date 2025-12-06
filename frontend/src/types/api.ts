export interface ApiError {
  message: string
  code?: string
  details?: unknown
}

export interface ListResponse<T> {
  data: T[]
  total: number
}
