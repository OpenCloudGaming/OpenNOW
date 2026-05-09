import type { JSX } from "react";
import { SHELF_IMAGE_PROPS } from "./constants";

interface DetailRailItem {
  id: string;
  title: string;
  subtitle: string;
  imageUrl?: string;
}

interface DetailRailProps {
  ps5Row: "top" | "main" | "detail";
  canEnterDetailRow: boolean;
  detailRailItems: DetailRailItem[];
  detailRailIndex: number;
}

export function DetailRail({
  ps5Row,
  canEnterDetailRow,
  detailRailItems,
  detailRailIndex,
}: DetailRailProps): JSX.Element | null {
  if (!(ps5Row === "detail" && canEnterDetailRow && detailRailItems.length > 0)) {
    return null;
  }

  return (
    <div className="xmb-ps5-detail-rail" role="listbox" aria-label="Detail row">
      {detailRailItems.map((item, idx) => (
        <div key={item.id} className={`xmb-ps5-detail-card ${idx === detailRailIndex ? "active" : ""}`} role="option" aria-selected={idx === detailRailIndex}>
          <div className="xmb-ps5-detail-card-image-wrap">
            {item.imageUrl ? (
              <img src={item.imageUrl} alt="" className="xmb-ps5-detail-card-image" {...SHELF_IMAGE_PROPS} />
            ) : (
              <div className="xmb-ps5-detail-card-image xmb-ps5-detail-card-image--placeholder" />
            )}
          </div>
          <div className="xmb-ps5-detail-card-title">{item.title}</div>
          <div className="xmb-ps5-detail-card-subtitle">{item.subtitle}</div>
        </div>
      ))}
    </div>
  );
}
