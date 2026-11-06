import { addEventListener, definePlugin, removeEventListener, routerHook, toaster } from "@decky/api";
import { InstallProgress } from "./backend";
import { OpenNOWPage } from "./components/Page";
import { QuickAccess } from "./components/QuickAccess";
import { TitleView } from "./components/TitleView";
import { patchGamePage } from "./gamePage";
import { ownedShortcutIds, syncShortcuts } from "./opennow";
import { ROUTE } from "./routes";
import { onAppLifetime } from "./steam";
import { refresh, store } from "./store";
import { OpenNOWIcon } from "./icon";

export default definePlugin(() => {
  routerHook.addRoute(ROUTE, OpenNOWPage);
  const unpatchGamePage = patchGamePage();
  const progress = addEventListener<[InstallProgress]>("install_progress", (installProgress) => {
    const status = store.get().status;
    if (status) store.set({ status: { ...status, installProgress } });
  });
  const done = addEventListener<[boolean, string]>("install_done", (ok, detail) => {
    if (ok || detail) toaster.toast({ title: "OpenNOW", body: ok ? "OpenNOW " + detail + " is installed" : detail });
    refresh();
    syncShortcuts().catch(console.error);
  });
  const stopLifetime = onAppLifetime((appId) => {
    const state = store.get().state;
    if (state && ownedShortcutIds(state).includes(appId)) setTimeout(refresh, 500);
  });
  syncShortcuts().catch(console.error);
  return {
    name: "OpenNOW",
    titleView: <TitleView />,
    content: <QuickAccess />,
    icon: <OpenNOWIcon />,
    onDismount() {
      routerHook.removeRoute(ROUTE);
      unpatchGamePage();
      removeEventListener("install_progress", progress);
      removeEventListener("install_done", done);
      stopLifetime();
    },
  };
});
