import { useState, useEffect, useRef, useCallback } from "react";
import { Form, Input, Select, Button } from "./Forms";
import type { InputRef } from "./Forms";
import { toTitleCase } from "../utils/formatters";
import { INTERVALS, KEY_CODES } from "../utils/constants";
import type { Entry, CreateEntryInput, Interval } from "../types/entry";
import styles from "../styles/Forms.module.css";

const isValidUrl = (str: string): boolean => {
  try {
    new URL(str);
    return true;
  } catch {
    return false;
  }
};

interface CreateEntryFormProps {
  onSubmit: (entry: CreateEntryInput) => void;
  entries: Entry[];
  // Optional props for editing existing entry
  id?: string;
  url?: string;
  title?: string;
  description?: string | null;
  duration?: number;
  interval?: Interval;
  dismissed_at?: string | null;
}

export default function CreateEntryForm({
  onSubmit,
  entries,
  id,
  url: initialUrl = "",
  title: initialTitle = "",
  description: initialDescription = "",
  duration: initialDuration = 3,
  interval: initialInterval = INTERVALS.DAYS,
  dismissed_at: initialDismissedAt = null,
}: CreateEntryFormProps) {
  const [url, setUrl] = useState(initialUrl);
  const [title, setTitle] = useState(initialTitle);
  const [description, setDescription] = useState(initialDescription || "");
  const [duration, setDuration] = useState(initialDuration.toString());
  const [interval, setInterval] = useState<Interval>(initialInterval);
  const [error, setError] = useState("");

  const urlInputRef = useRef<InputRef>(null);

  useEffect(() => {
    urlInputRef.current?.focus();
  }, []);

  const handleSubmit = useCallback(() => {
    if (!url || !title) {
      setError("URL and Title are required.");
      return;
    }

    if (!isValidUrl(url)) {
      setError("URL is invalid.");
      return;
    }

    if (!id) {
      const normalizedUrl = new URL(url).href;
      const urlExists = entries.some(
        (x) => new URL(x.url).href === normalizedUrl,
      );
      if (urlExists) {
        setError("URL already exists.");
        return;
      }
    }

    const durationNum = parseInt(duration, 10);
    if (!durationNum || durationNum < 1) {
      setError("Duration must be greater than 0.");
      return;
    }

    setError("");

    const entry: CreateEntryInput = {
      url: new URL(url).href,
      title,
      description: description || null,
      duration: durationNum,
      interval,
      dismissed_at: initialDismissedAt,
    };

    onSubmit(entry);

    // Reset form only if creating new entry (not editing)
    if (!id) {
      setUrl("");
      setTitle("");
      setDescription("");
      setDuration("3");
      setInterval(INTERVALS.DAYS);
    }
  }, [
    url,
    title,
    description,
    duration,
    interval,
    id,
    entries,
    initialDismissedAt,
    onSubmit,
  ]);

  useEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.keyCode === KEY_CODES.ENTER) {
        handleSubmit();
      }
    };

    document.addEventListener("keydown", handleKeydown);
    return () => document.removeEventListener("keydown", handleKeydown);
  }, [handleSubmit]);

  return (
    <Form>
      {!!error && <div className={styles.error}>{error}</div>}
      <Input
        type="url"
        ref={urlInputRef}
        value={url}
        label="URL"
        placeholder="http://example.com"
        onChange={setUrl}
      />
      <Input value={title} label="Title" onChange={setTitle} />
      <Input
        value={description}
        label="Description"
        onChange={setDescription}
      />
      <Input
        type="number"
        pattern="[0-9]*"
        value={duration}
        label="Duration"
        onChange={setDuration}
        min={1}
      />
      <Select
        label="Interval"
        value={interval}
        onChange={(val) => setInterval(val as Interval)}
        options={Object.keys(INTERVALS).map((key) => ({
          id: INTERVALS[key as keyof typeof INTERVALS],
          display: toTitleCase(key),
        }))}
      />
      <Button onClick={handleSubmit}>{id ? "Edit Entry" : "Add Entry"}</Button>
    </Form>
  );
}
