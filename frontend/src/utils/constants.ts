import type { Interval } from "../types/entry";

export const INTERVALS: Record<Uppercase<Interval>, Interval> = {
  HOURS: "hours",
  DAYS: "days",
  WEEKS: "weeks",
  MONTHS: "months",
  YEARS: "years",
};

export const MODES = {
  VIEW: "view",
  EDIT: "edit",
} as const;

export const KEY_CODES = {
  ESC: 27,
  FWD_SLASH: 191,
  ENTER: 13,
} as const;
