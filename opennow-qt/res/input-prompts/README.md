# Input prompt artwork

Selected vector artwork from the user-supplied Kenney Input Prompts 1.5 archive
(the included license identifies it as 1.5A). See `License.txt` and the root
`THIRD_PARTY_NOTICES` for attribution and CC0 licensing.

Source folders: `Xbox Series/Vector`, `PlayStation Series/Vector`, and
`Keyboard & Mouse/Vector`. The original SVG path geometry is unchanged.
Controller canvases are cropped from 64×64 to `viewBox="8 8 48 48"`;
keyboard canvases are cropped to their artwork bounds, preserving the wider
modifier key shapes. This removes outer padding at small UI sizes.
Files ending in `-dark.svg` replace white with `#171B26` for light surfaces.
Static variants avoid a shader or offscreen layer per icon.

`InputPromptIcons.qml` maps existing Xbox-style shell prompts to these assets.
MENU and VIEW identify controller buttons, keeping ordinary plus/minus action
symbols and keyboard keycaps separate. Device rows retain their reported
Xbox/PlayStation family; unknown devices use the generic controller silhouette.
This artwork change does not alter controller mappings or add automatic
button-family switching.

`KeyboardGlyph.qml` renders canonical keyboard shortcuts as individual key
images. Letter/number assets cover configurable shortcuts, not only the defaults.
Modifier aliases and both `Ctrl+K` and `Ctrl K` spellings are supported, including
literal plus keys (`Ctrl++`). Unknown keys/chords retain their exact text.
Translated accessible labels stay separate from canonical key identifiers.
On macOS the icons follow Qt's portable shortcut convention: Ctrl means Command,
Meta means Control, and Alt uses the Option symbol. Bindings are not changed.
