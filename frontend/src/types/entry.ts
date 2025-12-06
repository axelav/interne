export type Interval = 'hours' | 'days' | 'weeks' | 'months' | 'years'

export interface Entry {
  id: string
  user_id: string
  url: string
  title: string
  description: string | null
  duration: number
  interval: Interval
  visited: number
  created_at: string
  updated_at: string | null
  dismissed_at: string | null
  // Computed client-side
  visible?: boolean
  availableAt?: Date
}

export type CreateEntryInput = Omit<
  Entry,
  'id' | 'user_id' | 'created_at' | 'updated_at' | 'visited' | 'visible' | 'availableAt'
>

export type UpdateEntryInput = Partial<CreateEntryInput>
