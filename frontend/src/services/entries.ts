import type { Entry, CreateEntryInput, UpdateEntryInput } from "../types/entry";
import { pb } from "./pocketbase";

export async function fetchEntries(): Promise<Entry[]> {
  return pb.collection('entries').getFullList<Entry>({
    sort: '-created',
  });
}

export async function createEntry(input: CreateEntryInput): Promise<Entry> {
  const user = pb.authStore.model;
  if (!user) {
    throw new Error('Must be logged in to create entries');
  }

  return pb.collection('entries').create<Entry>({
    ...input,
    user: user.id,
    visited: 0,
  });
}

export async function updateEntry(id: string, updates: UpdateEntryInput): Promise<Entry> {
  return pb.collection('entries').update<Entry>(id, updates);
}

export async function deleteEntry(id: string): Promise<void> {
  await pb.collection('entries').delete(id);
}
