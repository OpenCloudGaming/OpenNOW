import { handleAllActivateAction, handleAllCancelAction, handleAllSecondaryAction } from "./allCategoryActions";
import { handleMediaActivateAction, handleMediaCancelAction } from "./mediaActions";
import { handleOptionsActivateAction, openOptionsMenuAction } from "./optionsActions";
import { handleSettingsActivateAction, handleSettingsCancelAction, handleSettingsSecondaryAction } from "./settingsActions";
import type {
  AllActivateContext,
  AllCancelContext,
  AllSecondaryContext,
  MediaActivateContext,
  MediaCancelContext,
  OptionsActionContext,
  SettingsActivateContext,
  SettingsCancelContext,
  SettingsSecondaryContext,
} from "./contracts";

export function routeOptionsActivate(
  context: Parameters<typeof handleOptionsActivateAction>[0] & { optionsOpen: boolean },
): boolean {
  if (!context.optionsOpen) return false;
  return handleOptionsActivateAction(context);
}

export function routeOpenOptions(context: OptionsActionContext): boolean {
  return openOptionsMenuAction(context);
}

export function routeCategoryActivate(context: {
  topCategory: "settings" | "media" | "all" | string;
  settings?: SettingsActivateContext;
  media?: MediaActivateContext;
  all?: AllActivateContext;
}): boolean {
  if (context.topCategory === "settings" && context.settings) {
    return handleSettingsActivateAction(context.settings);
  }
  if (context.topCategory === "media" && context.media) {
    return handleMediaActivateAction(context.media);
  }
  if (context.topCategory === "all" && context.all) {
    return handleAllActivateAction(context.all);
  }
  return false;
}

export function routeSecondaryActivate(context: {
  topCategory: "settings" | "all" | string;
  settings?: SettingsSecondaryContext;
  all?: AllSecondaryContext;
}): boolean {
  if (context.topCategory === "all" && context.all) {
    if (handleAllSecondaryAction(context.all)) return true;
  }
  if (context.topCategory === "settings" && context.settings) {
    return handleSettingsSecondaryAction(context.settings);
  }
  return false;
}

export function routeCancel(context: {
  topCategory: "settings" | "media" | "all" | string;
  settings?: SettingsCancelContext;
  media?: MediaCancelContext;
  all?: AllCancelContext;
}): boolean {
  if (context.topCategory === "settings" && context.settings) {
    return handleSettingsCancelAction(context.settings);
  }
  if (context.topCategory === "media" && context.media) {
    return handleMediaCancelAction(context.media);
  }
  if (
    context.all &&
    (context.topCategory === "all" ||
      (context.topCategory === "current" && context.all.gamesHubOpen))
  ) {
    return handleAllCancelAction(context.all);
  }
  return false;
}
