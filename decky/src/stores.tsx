import { FaCloud, FaSteam, FaXbox } from "react-icons/fa";
import { SiBattledotnet, SiEa, SiEpicgames, SiGogdotcom, SiUbisoft } from "react-icons/si";
import { IconType } from "react-icons";

const STORES: Record<string, [string, IconType]> = {
  STEAM: ["Steam", FaSteam],
  EPIC: ["Epic Games", SiEpicgames],
  UPLAY: ["Ubisoft Connect", SiUbisoft],
  EA_APP: ["EA app", SiEa],
  ORIGIN: ["EA app", SiEa],
  GOG: ["GOG", SiGogdotcom],
  BATTLENET: ["Battle.net", SiBattledotnet],
  XBOX: ["Xbox", FaXbox],
  NONE: ["Free to play", FaCloud],
};

export function storeLabel(store: string): string {
  return STORES[store]?.[0] ?? store.charAt(0) + store.slice(1).toLowerCase().split("_").join(" ");
}

export function StoreIcon({ store, size = 16 }: { store: string; size?: number }) {
  const Icon = STORES[store]?.[1] ?? FaCloud;
  return <Icon size={size} style={{ opacity: 0.7, flexShrink: 0 }} aria-label={storeLabel(store)} />;
}
