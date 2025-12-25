import type { Entry } from "../types/entry";
import type { Dayjs } from "dayjs";
import { getCurrentDate, getDate } from "./date";

const MAX = 7;
const MILLIS_IN_DAY = 24 * 60 * 60 * 1000;

// TODO: make user configurable
const opts = {
  entropy: 5,
};

export const getAvailableAtPlusEntropy = ({
  dismissed,
  interval,
  duration,
}: Pick<Entry, "dismissed" | "interval" | "duration">): {
  nextAvailable: Dayjs;
  diff: number;
} => {
  const now = getCurrentDate();
  const { entropy } = opts;

  const nextAvailable = dismissed
    ? getDate(dismissed).add(duration, interval as any)
    : now.subtract(1, "seconds");

  const diff = nextAvailable.diff(now);

  if (entropy && diff > MILLIS_IN_DAY) {
    const nextAvailablePlusEntropy = nextAvailable.add(
      Math.floor(Math.random() * ((entropy / 10) * MAX)),
      "days",
    );

    return {
      nextAvailable: nextAvailablePlusEntropy,
      diff: nextAvailablePlusEntropy.diff(now),
    };
  }

  return { nextAvailable, diff };
};
