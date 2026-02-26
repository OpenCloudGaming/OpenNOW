import {
  AlertTriangle,
  Code2,
  Gamepad2,
  Keyboard,
  MousePointer2,
  Palette,
  Play,
  Plus,
  Power,
  PowerOff,
  Trash2,
  WandSparkles,
} from "lucide-react";
import type { JSX } from "react";

import type { PluginRunState, PluginScript } from "../plugins";

interface PluginPageProps {
  plugins: PluginScript[];
  selectedPluginId: string;
  runStates: Record<string, PluginRunState>;
  inputReady: boolean;
  onSelectPlugin: (id: string) => void;
  onCreatePlugin: () => void;
  onDeletePlugin: (id: string) => void;
  onRunPlugin: (id: string) => void;
  onUpdatePlugin: (id: string, update: Partial<PluginScript>) => void;
}

const pluginTemplates = [
  {
    id: "keybind",
    label: "Keybind",
    icon: Keyboard,
    script: [
      "await input.keyTap(\"KeyK\", { ctrl: true, shift: true });",
      "log(\"Sent Ctrl+Shift+K to stream\");",
    ].join("\n"),
  },
  {
    id: "mouse",
    label: "Mouse Macro",
    icon: MousePointer2,
    script: [
      "await input.mouseMove(220, 0);",
      "await sleep(40);",
      "await input.mouseButton(\"left\", \"click\");",
      "log(\"Moved and clicked\");",
    ].join("\n"),
  },
  {
    id: "controller",
    label: "Controller",
    icon: Gamepad2,
    script: [
      "await input.controllerFrame({ buttons: input.buttons.A });",
      "await sleep(120);",
      "await input.controllerFrame({ buttons: 0 });",
      "log(\"Tapped gamepad A\");",
    ].join("\n"),
  },
  {
    id: "theme",
    label: "Theme",
    icon: Palette,
    script: [
      "await theme.setAccent(\"#4ade80\");",
      "log(\"Accent theme updated\");",
    ].join("\n"),
  },
] as const;

export function PluginPage({
  plugins,
  selectedPluginId,
  runStates,
  inputReady,
  onSelectPlugin,
  onCreatePlugin,
  onDeletePlugin,
  onRunPlugin,
  onUpdatePlugin,
}: PluginPageProps): JSX.Element {
  const selectedPlugin = plugins.find((plugin) => plugin.id === selectedPluginId) ?? null;
  const selectedRunState = selectedPlugin ? runStates[selectedPlugin.id] : undefined;

  return (
    <div className="plugin-page">
      <header className="plugin-header">
        <div>
          <h1>Plugins</h1>
          <p>Build custom scripts for keybinds, input automation, and theming.</p>
        </div>
        <div className="plugin-header-badges">
          <span className={`plugin-badge ${inputReady ? "is-live" : ""}`}>
            {inputReady ? "Input Ready" : "Input Offline"}
          </span>
          <span className="plugin-badge">{plugins.length} Installed</span>
        </div>
      </header>

      <div className="plugin-warning">
        <AlertTriangle size={16} />
        <p>Plugins run custom JavaScript. Only run scripts you trust.</p>
      </div>

      <div className="plugin-layout">
        <aside className="plugin-sidebar">
          <div className="plugin-sidebar-header">
            <h2>Scripts</h2>
            <button type="button" className="plugin-add-btn" onClick={onCreatePlugin}>
              <Plus size={15} />
              New
            </button>
          </div>

          {plugins.length === 0 ? (
            <div className="plugin-empty">
              <Code2 size={22} />
              <p>No plugins yet</p>
            </div>
          ) : (
            <div className="plugin-list">
              {plugins.map((plugin) => {
                const runState = runStates[plugin.id];
                return (
                  <div
                    key={plugin.id}
                    className={`plugin-list-item ${selectedPluginId === plugin.id ? "active" : ""}`}
                  >
                    <button type="button" className="plugin-list-select" onClick={() => onSelectPlugin(plugin.id)}>
                      <span className="plugin-list-name">{plugin.name}</span>
                      <span className="plugin-list-shortcut">{plugin.shortcut || "No shortcut"}</span>
                    </button>
                    <div className="plugin-list-actions">
                      {runState && runState.status !== "idle" && (
                        <span className={`plugin-run-chip plugin-run-chip--${runState.status}`}>{runState.status}</span>
                      )}
                      <button
                        type="button"
                        className="plugin-icon-btn"
                        title={plugin.enabled ? "Disable plugin" : "Enable plugin"}
                        onClick={() => onUpdatePlugin(plugin.id, { enabled: !plugin.enabled })}
                      >
                        {plugin.enabled ? <Power size={14} /> : <PowerOff size={14} />}
                      </button>
                      <button
                        type="button"
                        className="plugin-icon-btn"
                        title="Run plugin"
                        onClick={() => onRunPlugin(plugin.id)}
                      >
                        <Play size={14} />
                      </button>
                      <button
                        type="button"
                        className="plugin-icon-btn danger"
                        title="Delete plugin"
                        onClick={() => onDeletePlugin(plugin.id)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </aside>

        <section className="plugin-editor">
          {selectedPlugin ? (
            <>
              <div className="plugin-editor-grid">
                <label className="plugin-field">
                  <span>Name</span>
                  <input
                    type="text"
                    value={selectedPlugin.name}
                    onChange={(event) => onUpdatePlugin(selectedPlugin.id, { name: event.target.value })}
                  />
                </label>

                <label className="plugin-field">
                  <span>Shortcut</span>
                  <input
                    type="text"
                    placeholder="Ctrl+Shift+P"
                    value={selectedPlugin.shortcut}
                    onChange={(event) => onUpdatePlugin(selectedPlugin.id, { shortcut: event.target.value })}
                  />
                </label>
              </div>

              <label className="plugin-field">
                <span>Description</span>
                <input
                  type="text"
                  placeholder="What this plugin does"
                  value={selectedPlugin.description}
                  onChange={(event) => onUpdatePlugin(selectedPlugin.id, { description: event.target.value })}
                />
              </label>

              <div className="plugin-templates">
                <span>Templates</span>
                <div className="plugin-template-row">
                  {pluginTemplates.map((template) => {
                    const Icon = template.icon;
                    return (
                      <button
                        key={template.id}
                        type="button"
                        className="plugin-template-btn"
                        onClick={() => onUpdatePlugin(selectedPlugin.id, { script: template.script })}
                      >
                        <Icon size={14} />
                        {template.label}
                      </button>
                    );
                  })}
                </div>
              </div>

              <label className="plugin-field plugin-field--code">
                <span>Script</span>
                <textarea
                  spellCheck={false}
                  value={selectedPlugin.script}
                  onChange={(event) => onUpdatePlugin(selectedPlugin.id, { script: event.target.value })}
                />
              </label>

              <div className="plugin-editor-footer">
                <button
                  type="button"
                  className="plugin-run-btn"
                  onClick={() => onRunPlugin(selectedPlugin.id)}
                >
                  <WandSparkles size={15} />
                  Run Script
                </button>
                {selectedRunState?.message && (
                  <span className={`plugin-run-message plugin-run-message--${selectedRunState.status}`}>
                    {selectedRunState.message}
                  </span>
                )}
              </div>
            </>
          ) : (
            <div className="plugin-empty plugin-empty--editor">
              <Code2 size={24} />
              <p>Create a plugin to start scripting.</p>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
