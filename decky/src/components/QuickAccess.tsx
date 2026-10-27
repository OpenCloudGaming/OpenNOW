import { ButtonItem, Field, Navigation, PanelSection, PanelSectionRow, ProgressBarWithInfo } from "@decky/ui";
import { toaster } from "@decky/api";
import { ReactElement, useEffect, useState } from "react";
import { FaChevronRight, FaDownload, FaExternalLinkAlt, FaInfoCircle, FaPlay, FaSlidersH, FaTimes } from "react-icons/fa";
import { backend, InstallProgress, Status, StreamValues } from "../backend";
import { errorText, installSummary, phase, streamSummary } from "../format";
import { closeOpenNOW, launchGame, launchOpenNOW } from "../opennow";
import { ROUTE } from "../routes";
import { refresh, useSnapshot } from "../store";
import { StoreIcon } from "../stores";
import { Labelled } from "./StatusDot";

const QUICK_LAUNCH_LIMIT = 5;

function report(error: unknown) {
  toaster.toast({ title: "OpenNOW", body: errorText(error) });
}

export function openPage(page: string) {
  Navigation.Navigate(ROUTE + "/" + page);
  Navigation.CloseSideMenus();
}

function withIcon(icon: ReactElement, label: string) {
  return (
    <span style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 10 }}>
      {icon}
      {label}
    </span>
  );
}

function Chevron({ label, description, onActivate, icon }: { label: string; description?: string; onActivate: () => void; icon?: ReactElement }) {
  return (
    <Field label={label} description={description} icon={icon} onActivate={onActivate} highlightOnFocus focusable>
      <FaChevronRight size={14} style={{ opacity: 0.7 }} />
    </Field>
  );
}

export function InstallProgressRows({ progress }: { progress: InstallProgress }) {
  const downloading = progress.phase === "download";
  const measured = downloading || progress.fraction > 0;
  return (
    <>
      <PanelSectionRow>
        <ProgressBarWithInfo
          nProgress={measured ? progress.fraction * 100 : undefined}
          indeterminate={!measured}
          sOperationText={downloading ? "Downloading OpenNOW " + progress.version : "Installing OpenNOW " + progress.version}
          bottomSeparator="none"
        />
      </PanelSectionRow>
      {downloading && (
        <PanelSectionRow>
          <ButtonItem layout="below" onClick={() => backend.cancelInstall().then(refresh).catch(report)}>
            {withIcon(<FaTimes />, "Cancel")}
          </ButtonItem>
        </PanelSectionRow>
      )}
    </>
  );
}

function StatusRow({ status }: { status: Status }) {
  const current = phase(status);
  const install = status.install;
  if (current === "update" && install) {
    return (
      <Field label={<Labelled dot="blue">Update available</Labelled>} bottomSeparator="none">
        {install.version} → {status.release?.version}
      </Field>
    );
  }
  const labels = { running: ["green", "Running"], signedOut: ["yellow", "Not signed in"], ready: ["green", "Ready"] } as const;
  if (!install || !(current in labels)) return null;
  const [dot, text] = labels[current as keyof typeof labels];
  return (
    <Field label={<Labelled dot={dot}>{text}</Labelled>} bottomSeparator="none">
      {installSummary(install)}
    </Field>
  );
}

function Notice({ title, body }: { title: string; body: string }) {
  return (
    <PanelSectionRow>
      <Field label={title} description={body} icon={<FaInfoCircle />} bottomSeparator="none" />
    </PanelSectionRow>
  );
}

function Header({ status }: { status: Status }) {
  const current = phase(status);
  const install = (nightly?: boolean) => backend.installOpenNOW(nightly).then(refresh).catch(report);
  return (
    <PanelSection>
      {current === "missing" && (
        <>
          <Notice title="OpenNOW isn't installed" body="Install the official OpenNOW Flatpak to play GeForce NOW from Game Mode." />
          <PanelSectionRow>
            <ButtonItem layout="below" onClick={() => install(false)}>{withIcon(<FaDownload />, "Install OpenNOW")}</ButtonItem>
          </PanelSectionRow>
          <PanelSectionRow>
            <ButtonItem layout="below" description="Newest build from the dev branch. Updates follow nightlies too." onClick={() => install(true)}>
              {withIcon(<FaDownload />, "Install nightly")}
            </ButtonItem>
          </PanelSectionRow>
        </>
      )}
      {current === "installing" && status.installProgress && <InstallProgressRows progress={status.installProgress} />}
      {status.install && current !== "installing" && (
        <PanelSectionRow>
          <StatusRow status={status} />
        </PanelSectionRow>
      )}
      {current === "signedOut" && (
        <Notice title="Sign in from OpenNOW" body="OpenNOW handles your NVIDIA account. Launch it once and sign in there." />
      )}
      {current === "update" && (
        <PanelSectionRow>
          <ButtonItem layout="below" onClick={() => install()}>{withIcon(<FaDownload />, "Update to " + status.release?.version)}</ButtonItem>
        </PanelSectionRow>
      )}
      {current === "running" && (
        <>
          <PanelSectionRow>
            <ButtonItem layout="below" onClick={() => Navigation.CloseSideMenus()}>
              {withIcon(<FaExternalLinkAlt />, "Return to OpenNOW")}
            </ButtonItem>
          </PanelSectionRow>
          <PanelSectionRow>
            <ButtonItem layout="below" description="Stream settings are locked while OpenNOW runs." onClick={() => closeOpenNOW().catch(report)}>
              {withIcon(<FaTimes />, "Close OpenNOW")}
            </ButtonItem>
          </PanelSectionRow>
        </>
      )}
      {(current === "ready" || current === "signedOut" || current === "update") && (
        <PanelSectionRow>
          <ButtonItem layout="below" onClick={() => launchOpenNOW().catch(report)}>{withIcon(<FaPlay />, "Launch OpenNOW")}</ButtonItem>
        </PanelSectionRow>
      )}
    </PanelSection>
  );
}

function QuickLaunch() {
  const { state } = useSnapshot();
  const games = state?.games ?? [];
  return (
    <PanelSection title="Quick launch">
      {games.length === 0 && (
        <>
          <PanelSectionRow>
            <Field label="No games yet" description="Add GeForce NOW games to stream them straight from your library." bottomSeparator="none" />
          </PanelSectionRow>
          <PanelSectionRow>
            <Chevron label="Add games" onActivate={() => openPage("add")} />
          </PanelSectionRow>
        </>
      )}
      {games.slice(0, QUICK_LAUNCH_LIMIT).map((game) => (
        <PanelSectionRow key={game.appId}>
          <Field label={game.title} onActivate={() => launchGame(game.appId, game.title).catch(report)} highlightOnFocus focusable>
            <StoreIcon store={game.store} />
          </Field>
        </PanelSectionRow>
      ))}
      {games.length > 0 && (
        <PanelSectionRow>
          <Chevron label={"See all (" + games.length + ")"} onActivate={() => openPage("library")} />
        </PanelSectionRow>
      )}
    </PanelSection>
  );
}

function StreamRow() {
  const [values, setValues] = useState<StreamValues | null>(null);
  useEffect(() => {
    backend.getStreamSettings().then((settings) => setValues(settings.values)).catch(() => setValues(null));
  }, []);
  return (
    <PanelSection>
      <PanelSectionRow>
        <Chevron
          label="Stream settings"
          icon={<FaSlidersH />}
          description={values ? streamSummary(values) : undefined}
          onActivate={() => openPage("stream")}
        />
      </PanelSectionRow>
    </PanelSection>
  );
}

export function QuickAccess() {
  const { status } = useSnapshot();
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, []);
  if (!status) return null;
  const current = phase(status);
  return (
    <>
      <Header status={status} />
      {(current === "ready" || current === "running" || current === "update") && <QuickLaunch />}
      {(current === "ready" || current === "update") && <StreamRow />}
    </>
  );
}
