import {
  DialogButton,
  DialogControlsSection,
  DialogControlsSectionHeader,
  DropdownItem,
  Field,
  Focusable,
  SidebarNavigation,
  SliderField,
  Spinner,
  TextField,
  ToggleField,
} from "@decky/ui";
import { FileSelectionType, openFilePicker, toaster } from "@decky/api";
import { CSSProperties, ReactNode, useEffect, useRef, useState } from "react";
import { FaCheck, FaDownload, FaInfoCircle, FaLock, FaPlus, FaRedo, FaSearch, FaSlidersH, FaTh, FaTimes, FaTrash } from "react-icons/fa";
import { backend, CatalogGame, CatalogVariant, Codec, PreferenceKey, StreamSettings, StreamValues } from "../backend";
import { errorText, homePath, installKind, installSummary, resolutionLabel, settingsFile } from "../format";
import { addGame, closeOpenNOW, recreateLauncher, removeGame, syncShortcuts } from "../opennow";
import { ROUTE } from "../routes";
import { refresh, useSnapshot } from "../store";
import { StoreIcon, storeLabel } from "../stores";
import { InstallProgressRows } from "./QuickAccess";
import { StatusDot } from "./StatusDot";

const CODEC_LABELS: Record<Codec, string> = { auto: "Auto", av1: "AV1", h265: "H.265", h264: "H.264" };
const COMPACT: CSSProperties = { minWidth: 0, width: "auto", padding: "0 16px", display: "flex", alignItems: "center", gap: 8 };

function report(error: unknown) {
  toaster.toast({ title: "OpenNOW", body: errorText(error) });
}

function Section({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <DialogControlsSection>
      {title && <DialogControlsSectionHeader>{title}</DialogControlsSectionHeader>}
      {children}
    </DialogControlsSection>
  );
}

function Value({ label, value, description }: { label: string; value?: ReactNode; description?: string }) {
  return (
    <Field label={label} description={description} focusable>
      <span style={{ opacity: 0.7 }}>{value}</span>
    </Field>
  );
}

function GameList() {
  const { state } = useSnapshot();
  const [busy, setBusy] = useState<number | null>(null);
  if (!state?.games.length) return <Field description="Games you add show up here and in your Steam library." />;
  return (
    <>
      {state.games.map((game) => (
        <Field key={game.appId} label={game.title} childrenContainerWidth="max">
          <Focusable style={{ display: "flex", alignItems: "center", justifyContent: "flex-end", gap: 16 }}>
            <StoreIcon store={game.store} size={20} />
            <DialogButton
              style={COMPACT}
              disabled={busy === game.appId}
              onClick={() => {
                setBusy(game.appId);
                removeGame(game.appId).catch(report).finally(() => setBusy(null));
              }}
            >
              <FaTrash size={12} /> Remove
            </DialogButton>
          </Focusable>
        </Field>
      ))}
    </>
  );
}

function SearchResult({ game, added }: { game: CatalogGame; added: Set<string> }) {
  const [variant, setVariant] = useState<CatalogVariant>(game.variants.find((v) => !added.has(v.id)) ?? game.variants[0]);
  const [busy, setBusy] = useState(false);
  const inLibrary = added.has(variant.id);
  return (
    <Field
      label={game.title}
      icon={game.art.grid ? <img src={game.art.grid} width={60} height={85} style={{ objectFit: "cover", borderRadius: 2 }} alt="" /> : undefined}
      description={
        <Focusable style={{ display: "flex", flexWrap: "wrap", gap: 8, marginTop: 8 }}>
          {game.variants.map((option) => (
            <DialogButton
              key={option.id}
              onClick={() => setVariant(option)}
              style={{
                ...COMPACT,
                height: 36,
                borderRadius: 18,
                fontSize: 14,
                boxShadow: option.id === variant.id ? "inset 0 0 0 2px #1a9fff" : undefined,
              }}
            >
              <StoreIcon store={option.store} size={14} />
              {storeLabel(option.store)}
              {option.id === variant.id && <FaCheck size={10} />}
            </DialogButton>
          ))}
        </Focusable>
      }
    >
      <DialogButton
        style={COMPACT}
        disabled={busy || inLibrary}
        onClick={() => {
          setBusy(true);
          addGame(game, variant).catch(report).finally(() => setBusy(false));
        }}
      >
        {inLibrary ? <><FaCheck size={12} /> In library</> : busy ? "Adding…" : <><FaPlus size={12} /> Add to Library</>}
      </DialogButton>
    </Field>
  );
}

function AddGames() {
  const [query, setQuery] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [results, setResults] = useState<CatalogGame[]>([]);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const { state } = useSnapshot();
  const latest = useRef(0);
  useEffect(() => {
    const timer = setTimeout(() => setSubmitted(query), 400);
    return () => clearTimeout(timer);
  }, [query]);
  useEffect(() => {
    const request = ++latest.current;
    if (!submitted.trim()) {
      setResults([]);
      setError(null);
      return;
    }
    setSearching(true);
    backend.searchCatalog(submitted)
      .then((games) => request === latest.current && (setResults(games), setError(null)))
      .catch((failure) => request === latest.current && setError(errorText(failure)))
      .finally(() => request === latest.current && setSearching(false));
  }, [submitted, attempt]);
  const added = new Set(state?.games.map((game) => game.variantId));
  const retry = () => backend.refreshCatalog().then(() => setAttempt(attempt + 1)).catch((failure) => setError(errorText(failure)));
  return (
    <>
      <Focusable style={{ display: "flex", gap: 16, alignItems: "center" }}>
        <div style={{ flexGrow: 1 }}>
          <TextField
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => event.key === "Enter" && setSubmitted(query)}
          />
        </div>
        <DialogButton style={COMPACT} onClick={() => setSubmitted(query)}>
          <FaSearch size={12} /> Search
        </DialogButton>
      </Focusable>
      {error ? (
        <Section>
          <Field label="Can't reach the GeForce NOW catalog" description={error} icon={<FaInfoCircle />}>
            <DialogButton style={COMPACT} onClick={retry}>
              <FaRedo size={12} /> Try again
            </DialogButton>
          </Field>
        </Section>
      ) : submitted.trim() ? (
        <Section title={searching ? "GeForce NOW catalog" : "GeForce NOW catalog · " + results.length + (results.length === 1 ? " result" : " results")}>
          {searching && <Spinner style={{ height: 32, margin: "16px auto", display: "block" }} />}
          {!searching && results.length === 0 && <Field description={"No GeForce NOW games match \"" + submitted + "\"."} />}
          {!searching && results.map((game) => <SearchResult key={game.id} game={game} added={added} />)}
        </Section>
      ) : null}
      <Section title="Added to your library">
        <GameList />
      </Section>
    </>
  );
}

function Stream() {
  const [settings, setSettings] = useState<StreamSettings | null>(null);
  const { status } = useSnapshot();
  useEffect(() => {
    backend.getStreamSettings().then(setSettings).catch(report);
  }, [status?.running, status?.install?.data_dir]);
  if (!settings) return <Spinner style={{ height: 32 }} />;
  const { values, options, locked } = settings;
  const change = <K extends keyof StreamValues>(key: K, value: StreamValues[K]) => {
    setSettings({ ...settings, values: { ...values, [key]: value } });
    backend.setStreamSetting(key, value)
      .then((next) => setSettings({ ...settings, values: next }))
      .catch((error) => {
        report(error);
        backend.getStreamSettings().then(setSettings);
      });
  };
  const lock = (label: string) => (
    <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
      {label}
      {locked && <FaLock size={12} style={{ opacity: 0.6 }} />}
    </span>
  );
  return (
    <>
      {locked && (
        <Field
          label={status?.running ? "Locked while OpenNOW is running." : settings.error ?? "Install OpenNOW to change stream settings."}
          icon={<FaLock />}
        >
          {status?.running && (
            <DialogButton style={COMPACT} onClick={() => closeOpenNOW().catch(report)}>
              <FaTimes size={12} /> Close OpenNOW
            </DialogButton>
          )}
        </Field>
      )}
      <Section title="Picture">
        <DropdownItem
          label={lock("Resolution")}
          disabled={locked}
          rgOptions={options.resolutions.map((value) => ({ data: value, label: resolutionLabel(value) }))}
          selectedOption={values.resolution}
          onChange={(option) => change("resolution", option.data)}
        />
        <DropdownItem
          label={lock("Frame rate")}
          description={values.fps > 60 && !values.identifyAsSteamDeck ? "90 fps needs Steam Deck identity" : undefined}
          disabled={locked}
          rgOptions={options.frameRates.map((value) => ({ data: value, label: value + " fps" }))}
          selectedOption={values.fps}
          onChange={(option) => change("fps", option.data)}
        />
        <SliderField
          label={lock("Max bitrate")}
          disabled={locked}
          value={values.maxBitrateMbps}
          min={options.bitrate[0]}
          max={options.bitrate[1]}
          step={5}
          showValue
          valueSuffix=" Mbps"
          onChange={(value) => change("maxBitrateMbps", value)}
        />
        <DropdownItem
          label={lock("Codec")}
          disabled={locked}
          rgOptions={options.codecs.map((value) => ({ data: value, label: CODEC_LABELS[value] }))}
          selectedOption={values.codec}
          onChange={(option) => change("codec", option.data)}
        />
        <ToggleField
          label={lock("HDR")}
          description="Steam Deck OLED only"
          disabled={locked}
          checked={values.enableHdr}
          onChange={(value) => change("enableHdr", value)}
        />
      </Section>
      <Section title="Device">
        <ToggleField
          label={lock("Steam Deck identity")}
          description="Tells GeForce NOW this is a Steam Deck. Unlocks 90 fps."
          disabled={locked}
          checked={values.identifyAsSteamDeck}
          onChange={(value) => change("identifyAsSteamDeck", value)}
        />
        <ToggleField
          label={lock("Steam Big Picture mode")}
          description="Asks game launchers for their controller UI. Applies to new sessions."
          disabled={locked}
          checked={values.steamBigPictureMode}
          onChange={(value) => change("steamBigPictureMode", value)}
        />
      </Section>
    </>
  );
}

function Install() {
  const { status, state } = useSnapshot();
  const [checking, setChecking] = useState(false);
  useEffect(() => {
    if (status?.release) return;
    setChecking(true);
    backend.checkUpdate().then(refresh).catch(() => undefined).finally(() => setChecking(false));
  }, []);
  if (!status) return <Spinner style={{ height: 32 }} />;
  const install = status.install;
  const release = status.release;
  const runInstall = () => backend.installOpenNOW().then(refresh).catch(report);
  const pickPath = () =>
    openFilePicker(FileSelectionType.FILE, "/home")
      .then((result) => backend.setCustomPath(result.realpath))
      .then(syncShortcuts)
      .catch(report);
  const offer = !install
    ? { title: "OpenNOW isn't installed", action: "Install OpenNOW" }
    : release?.updateAvailable
      ? { title: "OpenNOW " + release.version + " is available", action: "Update to " + release.version }
      : install.kind === "flatpak" && release?.asset && !release.installedVersionKnown
        ? { title: "Latest release is " + release.version, action: "Install OpenNOW " + release.version }
        : null;
  return (
    <>
      {status.installProgress ? (
        <Section title="Installing">
          <InstallProgressRows progress={status.installProgress} />
        </Section>
      ) : offer && (
        <Section title={install ? "Update" : "Install"}>
          <Field label={<span style={{ display: "flex", alignItems: "center" }}><StatusDot colour="blue" />{offer.title}</span>} bottomSeparator="none" />
          <Field
            childrenLayout="below"
            description="Downloads the official Flatpak bundle from the OpenNOW GitHub release."
          >
            <DialogButton onClick={runInstall} disabled={status.running && !!install} style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 8 }}>
              <FaDownload size={14} /> {offer.action}
            </DialogButton>
          </Field>
        </Section>
      )}
      {install && (
        <Section title="Installed">
          <Value label="Install type" value={installKind(install)} />
          <Value label="Version" value={install.version ?? "Unknown"} />
          <Field label="Location" description={homePath(install.path)} childrenContainerWidth="min">
            <Focusable style={{ display: "flex", gap: 8 }}>
              {state?.customPath && (
                <DialogButton style={COMPACT} onClick={() => backend.setCustomPath("").then(syncShortcuts).catch(report)}>
                  Detect
                </DialogButton>
              )}
              <DialogButton style={COMPACT} onClick={pickPath}>Choose…</DialogButton>
            </Focusable>
          </Field>
          <Value label="Settings file" description={settingsFile(install)} />
        </Section>
      )}
      <Section>
        <Field label="Updates" description={release ? (offer ? undefined : "Up to date") : undefined}>
          <DialogButton
            style={COMPACT}
            disabled={checking}
            onClick={() => {
              setChecking(true);
              backend.checkUpdate().then(refresh).catch(report).finally(() => setChecking(false));
            }}
          >
            <FaRedo size={12} /> {checking ? "Checking…" : "Check now"}
          </DialogButton>
        </Field>
        <ToggleField
          label="Nightly builds"
          description="Installs and updates to the newest nightly from the dev branch instead of the stable release."
          checked={state?.preferences.nightlyChannel ?? false}
          disabled={checking || !!status.installProgress}
          onChange={(value) => {
            setChecking(true);
            backend.setPreference("nightlyChannel", value).then(() => backend.checkUpdate()).then(refresh).catch(report)
              .finally(() => setChecking(false));
          }}
        />
      </Section>
    </>
  );
}

const LAUNCH_PREFERENCES: [PreferenceKey, string, string][] = [
  ["consoleLayout", "Open the Game Mode UI on launch", "Starts OpenNOW in its controller UI (--console)."],
  ["gamePageButton", "Play on OpenNOW button", "Shows it on game pages for games that are on GeForce NOW."],
];

function Library() {
  const { state } = useSnapshot();
  const [busy, setBusy] = useState(false);
  const preference = (key: PreferenceKey, value: boolean) =>
    backend.setPreference(key, value).then(key === "consoleLayout" ? syncShortcuts : refresh).catch(report);
  return (
    <>
      <Section title="Library shortcut">
        <Field label="OpenNOW in your Steam library" description="Non-Steam shortcut that opens OpenNOW">
          <DialogButton
            style={COMPACT}
            disabled={busy}
            onClick={() => {
              setBusy(true);
              recreateLauncher().catch(report).finally(() => setBusy(false));
            }}
          >
            <FaRedo size={12} /> Re-create
          </DialogButton>
        </Field>
        <ToggleField
          label={"Keep games in an \"OpenNOW\" collection"}
          checked={state?.preferences.collection ?? true}
          onChange={(value) => preference("collection", value)}
        />
      </Section>
      <Section title="Launch">
        {LAUNCH_PREFERENCES.map(([key, label, description]) => (
          <ToggleField key={key} label={label} description={description} checked={state?.preferences[key] ?? true}
            onChange={(value) => preference(key, value)} />
        ))}
      </Section>
      <Section title={"Your OpenNOW games" + (state?.games.length ? " · " + state.games.length : "")}>
        <GameList />
      </Section>
    </>
  );
}

function About() {
  const { status } = useSnapshot();
  const [refreshing, setRefreshing] = useState(false);
  return (
    <Section title="OpenNOW for Decky">
      <Value label="Plugin version" value={status?.pluginVersion} />
      <Value label="OpenNOW" value={status?.install ? installSummary(status.install) : "Not installed"} />
      <Value label="Plugin log" value={status ? homePath(status.logPath) : undefined} />
      <Value label="Source" value="github.com/OpenCloudGaming/OpenNOW" />
      <Field label="GeForce NOW catalog">
        <DialogButton
          style={COMPACT}
          disabled={refreshing}
          onClick={() => {
            setRefreshing(true);
            backend.refreshCatalog()
              .then((count) => toaster.toast({ title: "OpenNOW", body: count.toLocaleString() + " games in the catalog" }))
              .catch(report)
              .finally(() => setRefreshing(false));
          }}
        >
          <FaRedo size={12} /> {refreshing ? "Refreshing…" : "Refresh"}
        </DialogButton>
      </Field>
    </Section>
  );
}

export function OpenNOWPage() {
  useEffect(() => {
    refresh();
    const timer = setInterval(refresh, 3000);
    return () => clearInterval(timer);
  }, []);
  return (
    <SidebarNavigation
      title="OpenNOW"
      showTitle
      pages={[
        { title: "Add games", icon: <FaSearch />, route: ROUTE + "/add", content: <AddGames /> },
        "separator",
        { title: "Stream", icon: <FaSlidersH />, route: ROUTE + "/stream", content: <Stream /> },
        { title: "Install", icon: <FaDownload />, route: ROUTE + "/install", content: <Install /> },
        { title: "Library", icon: <FaTh />, route: ROUTE + "/library", content: <Library /> },
        { title: "About", icon: <FaInfoCircle />, route: ROUTE + "/about", content: <About /> },
      ]}
    />
  );
}
