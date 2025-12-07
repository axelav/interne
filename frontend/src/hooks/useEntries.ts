import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as entriesService from "../services/entries";
import type { CreateEntryInput, UpdateEntryInput } from "../types/entry";

export function useEntries() {
  return useQuery({
    queryKey: ["entries"],
    queryFn: entriesService.fetchEntries,
  });
}

export function useCreateEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: CreateEntryInput) => entriesService.createEntry(input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    },
  });
}

export function useUpdateEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ id, updates }: { id: string; updates: UpdateEntryInput }) =>
      entriesService.updateEntry(id, updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    },
  });
}

export function useDeleteEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => entriesService.deleteEntry(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["entries"] });
    },
  });
}
