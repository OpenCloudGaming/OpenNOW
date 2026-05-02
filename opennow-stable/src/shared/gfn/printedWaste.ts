/** A single zone entry from the PrintedWaste queue API */
export interface PrintedWasteZone {
  QueuePosition: number;
  /** Unix timestamp of last update */
  "Last Updated": number;
  /** Geographic region code: "US" | "EU" | "JP" | "KR" | "CA" | "THAI" | "MY" */
  Region: string;
  /** Estimated wait time in milliseconds */
  eta?: number;
}

/** Full data payload from https://api.printedwaste.com/gfn/queue/ */
export type PrintedWasteQueueData = Record<string, PrintedWasteZone>;

/** PrintedWaste server metadata entry from remote mapping config */
export interface PrintedWasteServerMappingEntry {
  title?: string;
  region?: string;
  is4080Server?: boolean;
  is5080Server?: boolean;
  nuked?: boolean;
}

/** Full data payload from PrintedWaste server-to-region mapping config */
export type PrintedWasteServerMapping = Record<string, PrintedWasteServerMappingEntry>;
