import { Capacitor } from "@capacitor/core";

import { capacitorPlatform } from "./android/api";
import { electronPlatform } from "./electron";

const selectedPlatform = Capacitor.isNativePlatform() ? capacitorPlatform : electronPlatform;

export const openNow = selectedPlatform.api;
export const platform = selectedPlatform.info;
export const platformCapabilities = selectedPlatform.info.capabilities;
