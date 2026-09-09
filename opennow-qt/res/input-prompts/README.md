# Controller artwork

Selected vector artwork from the user-supplied Kenney Input Prompts 1.5 archive
(the included license identifies it as 1.5A). See `License.txt` and the root
`THIRD_PARTY_NOTICES` for attribution and CC0 licensing.

Source folders: `Xbox Series/Vector` and `PlayStation Series/Vector`.
The original SVG path geometry is unchanged. The canvas is cropped from 64×64 to
`viewBox="8 8 48 48"` to remove the pack's outer padding at small UI sizes.
Files ending in `-dark.svg` replace white with `#171B26` for light surfaces.
Static variants avoid a shader or offscreen layer per icon.

`ControllerIcons.qml` maps existing Xbox-style shell prompts to these assets.
MENU and VIEW identify controller buttons, keeping ordinary plus/minus action
symbols and keyboard keycaps separate. Device rows retain their reported
Xbox/PlayStation family; unknown devices use the generic controller silhouette.
This artwork change does not alter controller mappings or add automatic
button-family switching.
