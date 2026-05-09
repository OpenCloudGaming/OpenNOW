import type { ControllerThemeStyle } from "@shared/gfn";
import type {
  SettingsActivateContext,
  SettingsCancelContext,
  SettingsSecondaryContext,
} from "./contracts";

export function handleSettingsActivateAction(context: SettingsActivateContext): boolean {
  const {
    settingsSubcategory,
    displayItems,
    selectedSettingIndex,
    setLastRootSettingIndex,
    setSettingsSubcategory,
    setSelectedSettingIndex,
    onExitApp,
    setLastSystemMenuIndex,
    setEditingThemeChannel,
    setLastThemeRootIndex,
    sanitizeControllerThemeStyle,
    settings,
    themeStyleOrder,
    onSettingChange,
    onExitControllerMode,
    currentStreamingGame,
    setCategoryIndex,
    playUiSound,
  } = context;

  const setting = displayItems[selectedSettingIndex];
  if (settingsSubcategory === "root" && setting && (setting.id === "network" || setting.id === "audio" || setting.id === "video" || setting.id === "system")) {
    setLastRootSettingIndex(selectedSettingIndex);
    if (setting.id === "network") setSettingsSubcategory("Network");
    if (setting.id === "audio") setSettingsSubcategory("Audio");
    if (setting.id === "video") setSettingsSubcategory("Video");
    if (setting.id === "system") setSettingsSubcategory("System");
    setSelectedSettingIndex(0);
    playUiSound("confirm");
    return true;
  }
  if (settingsSubcategory === "root" && setting?.id === "exitApp") {
    if (onExitApp) {
      onExitApp();
    } else if (window.openNow?.quitApp) {
      void window.openNow.quitApp();
    }
    playUiSound("confirm");
    return true;
  }
  if (settingsSubcategory === "System" && setting?.id === "theme") {
    setLastSystemMenuIndex(selectedSettingIndex);
    setSettingsSubcategory("Theme");
    setSelectedSettingIndex(0);
    setEditingThemeChannel(null);
    playUiSound("confirm");
    return true;
  }
  if (settingsSubcategory === "Theme") {
    const item = displayItems[selectedSettingIndex];
    if (item?.id === "libraryGameBackdrop" && onSettingChange) {
      onSettingChange(
        "controllerLibraryGameBackdrop" as never,
        !(settings.controllerLibraryGameBackdrop !== false) as never,
      );
      playUiSound("confirm");
      return true;
    }
    if (item?.id === "themeColor") {
      setLastThemeRootIndex(selectedSettingIndex);
      setSettingsSubcategory("ThemeColor");
      setSelectedSettingIndex(0);
      setEditingThemeChannel(null);
      playUiSound("confirm");
      return true;
    }
    if (item?.id === "themeStyle") {
      setLastThemeRootIndex(selectedSettingIndex);
      setSettingsSubcategory("ThemeStyle");
      const resolvedStyle = sanitizeControllerThemeStyle(settings.controllerThemeStyle);
      const idx = themeStyleOrder.indexOf(resolvedStyle);
      setSelectedSettingIndex(idx >= 0 ? idx : 0);
      playUiSound("confirm");
      return true;
    }
    return true;
  }
  if (settingsSubcategory === "ThemeStyle") {
    const row = displayItems[selectedSettingIndex];
    if (row?.id && onSettingChange) {
      onSettingChange("controllerThemeStyle", row.id as ControllerThemeStyle);
      playUiSound("confirm");
    }
    return true;
  }
  if (settingsSubcategory !== "root") {
    if (setting?.id === "exitControllerMode") {
      if (onExitControllerMode) {
        onExitControllerMode();
      } else if (onSettingChange) {
        onSettingChange("controllerMode" as never, false as never);
      }
      playUiSound("confirm");
      const nextSettingsIndex = currentStreamingGame ? 0 : 1;
      setCategoryIndex(() => nextSettingsIndex);
      setSelectedSettingIndex(0);
      return true;
    }
    return false;
  }
  playUiSound("confirm");
  return true;
}

export function handleSettingsSecondaryAction(context: SettingsSecondaryContext): boolean {
  const {
    settingsSubcategory,
    displayItems,
    selectedSettingIndex,
    onSettingChange,
    settings,
    microphoneDevices,
    aspectRatioOptions,
    resolutionOptions,
    fpsOptions,
    codecOptions,
    setEditingThemeChannel,
    setEditingBandwidth,
    playUiSound,
  } = context;

  if (settingsSubcategory === "ThemeStyle" || settingsSubcategory === "Theme") return true;
  const setting = displayItems[selectedSettingIndex];
  if (!setting || !onSettingChange) return true;
  if (setting.id === "exitApp" || setting.id === "exitControllerMode") return true;
  if (settingsSubcategory === "root" && (setting.id === "network" || setting.id === "audio" || setting.id === "video" || setting.id === "system")) return true;

  if (
    settingsSubcategory === "ThemeColor" &&
    (setting.id === "themeR" || setting.id === "themeG" || setting.id === "themeB")
  ) {
    const channel = setting.id === "themeR" ? "r" : setting.id === "themeG" ? "g" : "b";
    setEditingThemeChannel(channel);
    playUiSound("move");
    return true;
  }

  if (setting.id === "microphone") {
    const current = settings.microphoneDeviceId;
    const list = microphoneDevices.length > 0 ? microphoneDevices : [{ deviceId: "", label: "Default" }];
    const ids = list.map((device) => device.deviceId);
    const curIdx = ids.indexOf(current ?? "");
    const nextIdx = (curIdx + 1) % ids.length;
    onSettingChange("microphoneDeviceId" as never, ids[nextIdx] as never);
    playUiSound("move");
    return true;
  }

  if (setting.id === "aspectRatio" && aspectRatioOptions.length > 0) {
    const currentIdx = aspectRatioOptions.indexOf(settings.aspectRatio || "16:9");
    const nextIdx = (currentIdx + 1) % aspectRatioOptions.length;
    onSettingChange("aspectRatio", aspectRatioOptions[nextIdx] as never);
    playUiSound("move");
  } else if (setting.id === "resolution" && resolutionOptions.length > 0) {
    const currentIdx = resolutionOptions.indexOf(settings.resolution || "1920x1080");
    const nextIdx = (currentIdx + 1) % resolutionOptions.length;
    onSettingChange("resolution", resolutionOptions[nextIdx] as never);
    playUiSound("move");
  } else if (setting.id === "fps" && fpsOptions.length > 0) {
    const currentIdx = fpsOptions.indexOf(settings.fps || 60);
    const nextIdx = (currentIdx + 1) % fpsOptions.length;
    onSettingChange("fps", fpsOptions[nextIdx] as never);
    playUiSound("move");
  } else if (setting.id === "codec" && codecOptions.length > 0) {
    const currentIdx = codecOptions.indexOf(settings.codec || "H264");
    const nextIdx = (currentIdx + 1) % codecOptions.length;
    onSettingChange("codec" as never, codecOptions[nextIdx] as never);
    playUiSound("move");
  } else if (setting.id === "sounds") {
    onSettingChange("controllerUiSounds", !(settings.controllerUiSounds || false) as never);
    playUiSound("move");
  } else if (setting.id === "autoLoad") {
    onSettingChange("autoLoadControllerLibrary" as never, !Boolean(settings.autoLoadControllerLibrary) as never);
    playUiSound("move");
  } else if (setting.id === "autoFullScreen") {
    onSettingChange("autoFullScreen" as never, !Boolean(settings.autoFullScreen) as never);
    playUiSound("move");
  } else if (setting.id === "backgroundAnimations") {
    onSettingChange("controllerBackgroundAnimations" as never, !Boolean(settings.controllerBackgroundAnimations) as never);
    playUiSound("move");
  } else if (setting.id === "l4s") {
    onSettingChange("enableL4S" as never, !Boolean(settings.enableL4S) as never);
    playUiSound("move");
  } else if (setting.id === "cloudGsync") {
    onSettingChange("enableCloudGsync" as never, !Boolean(settings.enableCloudGsync) as never);
    playUiSound("move");
  } else if (setting.id === "bandwidth") {
    setEditingBandwidth(true);
    playUiSound("move");
  }
  return true;
}

export function handleSettingsCancelAction(context: SettingsCancelContext): boolean {
  const {
    editingBandwidth,
    editingThemeChannel,
    settingsSubcategory,
    lastThemeRootIndex,
    lastSystemMenuIndex,
    lastRootSettingIndex,
    setEditingBandwidth,
    setEditingThemeChannel,
    setSettingsSubcategory,
    setSelectedSettingIndex,
    playUiSound,
  } = context;

  if (editingBandwidth) {
    setEditingBandwidth(false);
    playUiSound("move");
    return true;
  }
  if (editingThemeChannel) {
    setEditingThemeChannel(null);
    playUiSound("move");
    return true;
  }
  if (settingsSubcategory === "ThemeColor") {
    setSettingsSubcategory("Theme");
    setSelectedSettingIndex(lastThemeRootIndex);
    playUiSound("move");
    return true;
  }
  if (settingsSubcategory === "ThemeStyle") {
    setSettingsSubcategory("Theme");
    setSelectedSettingIndex(lastThemeRootIndex);
    playUiSound("move");
    return true;
  }
  if (settingsSubcategory === "Theme") {
    setSettingsSubcategory("System");
    setSelectedSettingIndex(lastSystemMenuIndex);
    playUiSound("move");
    return true;
  }
  setSettingsSubcategory("root");
  setSelectedSettingIndex(lastRootSettingIndex);
  playUiSound("move");
  return true;
}
