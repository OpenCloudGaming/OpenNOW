import {
  afterPatch,
  createReactTreePatcher,
  DialogButton,
  findClassModule,
  findInReactTree,
  Menu,
  MenuItem,
  showContextMenu,
  SteamSpinner,
} from "@decky/ui";
import { routerHook, toaster } from "@decky/api";
import { ReactElement, useEffect, useState } from "react";
import { FaCloud } from "react-icons/fa";
import { backend, CatalogMatch } from "./backend";
import { errorText } from "./format";
import { ownedShortcutIds, playOnOpenNOW } from "./opennow";
import { refresh, useSnapshot } from "./store";
import { StoreIcon, storeLabel } from "./stores";

const ROUTE = "/library/app/:appid";
const STEAM_GAME = 1;
const SHORTCUT = 1073741824;
const BUTTON_KEY = "opennow-play";

const STYLE = `
.opennow-play {
  display: flex !important;
  align-items: center;
  gap: 14px;
  align-self: stretch;
  min-height: 0;
  min-width: 220px !important;
  width: auto !important;
  margin: 0 0 0 16px !important;
  padding: 0 20px !important;
  border-radius: 2px !important;
  text-align: left;
  flex-shrink: 0;
  overflow: hidden;
}
.opennow-play .opennow-play-text { display: flex; flex-direction: column; line-height: 1.15; }
.opennow-play .opennow-play-title { font-size: 16px; font-weight: 500; }
.opennow-play .opennow-play-detail { font-size: 12px; opacity: 0.75; }
.opennow-play .opennow-play-progress { position: absolute; left: 0; bottom: 0; height: 5px; background: #1a9fff; }
`;

interface PageApp {
  appId: number;
  shortcutTitle: string;
}

function useMatches(appId: number, shortcutTitle: string): CatalogMatch[] {
  const [matches, setMatches] = useState<CatalogMatch[]>([]);
  useEffect(() => {
    backend.gamePageMatches(appId, shortcutTitle).then(setMatches).catch(() => setMatches([]));
  }, [appId, shortcutTitle]);
  return matches;
}

function detailLabel(matches: CatalogMatch[], busy: boolean, installed: boolean, progress?: { phase: string; fraction: number } | null): string {
  if (progress) return progress.phase === "download" || progress.fraction > 0 ? "Installing " + Math.round(progress.fraction * 100) + "%" : "Installing…";
  if (!installed) return "Install OpenNOW";
  if (busy) return "Starting…";
  if (matches.length > 1) return "Choose a version";
  return storeLabel(matches[0].variant.store) + " version";
}

function PlayOnOpenNOW({ appId, shortcutTitle }: PageApp) {
  const matches = useMatches(appId, shortcutTitle);
  const [busy, setBusy] = useState(false);
  const { status, state } = useSnapshot();
  if (!matches.length || !state?.preferences.gamePageButton || ownedShortcutIds(state).includes(appId)) return null;
  const run = (task: () => Promise<unknown>) => {
    setBusy(true);
    task()
      .catch((error) => toaster.toast({ title: "OpenNOW", body: errorText(error) }))
      .finally(() => setBusy(false));
  };
  const progress = status?.installProgress;
  const onClick = (event: MouseEvent) => {
    if (!status?.install) {
      run(() => backend.installOpenNOW().then(refresh));
      return;
    }
    if (matches.length === 1) {
      run(() => playOnOpenNOW(matches[0].game, matches[0].variant));
      return;
    }
    showContextMenu(
      <Menu label={"Stream " + matches[0].game.title + " from"}>
        {matches.map((match) => (
          <MenuItem key={match.variant.id} onSelected={() => run(() => playOnOpenNOW(match.game, match.variant))}>
            <span style={{ display: "flex", alignItems: "center", gap: 12 }}>
              <StoreIcon store={match.variant.store} size={18} />
              <span style={{ display: "flex", flexDirection: "column" }}>
                {storeLabel(match.variant.store)}
                {!shortcutTitle && match.variant.store === "STEAM" && (
                  <span style={{ fontSize: 12, opacity: 0.7 }}>In your Steam library</span>
                )}
              </span>
            </span>
          </MenuItem>
        ))}
      </Menu>,
      (event.currentTarget as EventTarget | null) ?? window,
    );
  };
  return (
    <DialogButton className="opennow-play" disabled={busy || !!progress} onClick={onClick} style={{ position: "relative" }}>
      <style>{STYLE}</style>
      {busy ? <SteamSpinner style={{ width: 22, height: 22, minHeight: 0, background: "none" }} /> : <FaCloud size={22} />}
      <span className="opennow-play-text">
        <span className="opennow-play-title">Play on OpenNOW</span>
        <span className="opennow-play-detail">{detailLabel(matches, busy, !!status?.install, progress)}</span>
      </span>
      {progress?.phase === "download" && <span className="opennow-play-progress" style={{ width: progress.fraction * 100 + "%" }} />}
    </DialogButton>
  );
}

function pageApp(overview: any): PageApp | undefined {
  if (typeof overview?.appid !== "number") return undefined;
  if (overview.app_type === STEAM_GAME) return { appId: overview.appid, shortcutTitle: "" };
  if (overview.app_type === SHORTCUT && typeof overview.display_name === "string") {
    return { appId: overview.appid, shortcutTitle: overview.display_name };
  }
  return undefined;
}

function hasClass(node: any, className: string): boolean {
  return typeof node?.props?.className === "string" && node.props.className.split(" ").includes(className);
}

function rendersClass(node: any, key: string): boolean {
  return typeof node?.type?.render === "function" && node.type.render.toString().includes("." + key + ",");
}

export function patchGamePage(): () => void {
  const actionRow = findClassModule((module: any) => module.ActionRow && module.AppButtons && module.PlaySection)?.ActionRow;
  if (!actionRow) return () => {};
  const sectionsPatcher = createReactTreePatcher([
    (section: any) => section,
    (rendered: any) => findInReactTree(rendered, (node: any) => rendersClass(node, "PlaySection")),
    (rendered: any) => findInReactTree(rendered, (node: any) => rendersClass(node, "ActionRow")),
  ], ([props]: any[], rendered?: ReactElement) => {
    const app = pageApp(props?.overview);
    const row = app && findInReactTree(rendered, (node: any) => Array.isArray(node?.props?.children) && hasClass(node, actionRow));
    if (!row || row.props.children.some((child: any) => child?.key === BUTTON_KEY)) return rendered;
    row.props.children.splice(1, 0, <PlayOnOpenNOW key={BUTTON_KEY} {...app} />);
    return rendered;
  }, "OpenNOW play bar");
  const patch = routerHook.addPatch(ROUTE, (tree: any) => {
    const routeProps = findInReactTree(tree, (node: any) => node?.renderFunc);
    if (!routeProps) return tree;
    afterPatch(routeProps, "renderFunc", (_: unknown, rendered: any) => {
      const sections = findInReactTree(rendered, (node: any) =>
        typeof node?.props?.value === "function" && node?.props?.children?.props?.overview);
      if (!sections) return rendered;
      const section = { type: sections.props.value };
      sectionsPatcher([], section);
      sections.props.value = section.type;
      return rendered;
    });
    return tree;
  });
  return () => routerHook.removePatch(ROUTE, patch);
}
