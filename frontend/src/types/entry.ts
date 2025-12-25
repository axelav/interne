export type Interval = "hours" | "days" | "weeks" | "months" | "years";

export interface Entry {
  id: string;
  user: string;
  url: string;
  title: string;
  description: string;
  duration: number;
  interval: Interval;
  visited: number;
  created: string;
  updated: string;
  dismissed: string;
  collectionId: string;
  collectionName: string;
  // Computed client-side
  visible?: boolean;
  nextAvailable?: Date;
}

export type CreateEntryInput = {
  url: string;
  title: string;
  description?: string;
  duration: number;
  interval: Interval;
  dismissed?: string;
};

export type UpdateEntryInput = Partial<CreateEntryInput> & {
  visited?: number;
};
