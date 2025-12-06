import type { Entry, CreateEntryInput, UpdateEntryInput } from "../types/entry";
import type { ListResponse } from "../types/api";
import { trailbase } from "./trailbase";

export async function fetchEntries(): Promise<Entry[]> {
  const response = await trailbase.request<ListResponse<Entry>>(
    "/records/v1/entries",
  );
  return response.data;
}

export async function createEntry(input: CreateEntryInput): Promise<Entry> {
  return trailbase.request<Entry>("/records/v1/entries", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function updateEntry(
  id: string,
  updates: UpdateEntryInput,
): Promise<Entry> {
  return trailbase.request<Entry>(`/records/v1/entries/${id}`, {
    method: "PATCH",
    body: JSON.stringify(updates),
  });
}

export async function deleteEntry(id: string): Promise<void> {
  await trailbase.request<void>(`/records/v1/entries/${id}`, {
    method: "DELETE",
  });
}
