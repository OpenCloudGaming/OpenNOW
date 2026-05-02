import type { MainIpcDeps } from "./types";
import { registerAuthSubscriptionGamesIpc } from "./registerAuthSubscriptionGames";
import { registerSessionIpc } from "./registerSessionIpc";
import { registerSignalingIpc } from "./registerSignalingIpc";
import { registerWindowUpdaterSettingsIpc } from "./registerWindowUpdaterSettingsIpc";
import { registerMediaIpc } from "./registerMediaIpc";
import { registerMiscIpc } from "./registerMiscIpc";

export function registerMainIpcHandlers(deps: MainIpcDeps): void {
  registerAuthSubscriptionGamesIpc(deps);
  registerSessionIpc(deps);
  registerSignalingIpc(deps);
  registerWindowUpdaterSettingsIpc(deps);
  registerMediaIpc(deps);
  registerMiscIpc(deps);
}
