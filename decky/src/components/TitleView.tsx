import { DialogButton, Focusable, staticClasses } from "@decky/ui";
import { FaCog } from "react-icons/fa";
import mark from "../../assets/mark.png";
import { openPage } from "./QuickAccess";

export function TitleView() {
  return (
    <Focusable style={{ display: "flex", alignItems: "center", width: "100%" }} className={staticClasses.Title}>
      <img src={mark} width={24} height={14} style={{ marginRight: 8 }} alt="" />
      <div style={{ flexGrow: 1 }}>OpenNOW</div>
      <DialogButton
        style={{ height: 28, width: 40, minWidth: 0, padding: "10px 12px", display: "flex", alignItems: "center", justifyContent: "center" }}
        onClick={() => openPage("stream")}
        aria-label="OpenNOW settings"
      >
        <FaCog size={14} />
      </DialogButton>
    </Focusable>
  );
}
