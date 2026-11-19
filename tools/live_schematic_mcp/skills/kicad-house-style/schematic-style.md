# Schematic style

Embeint schematic capture conventions.

## Blocks

- Organise every sheet into **logical blocks**: a dashed rectangle with a bold title per function, e.g.
  "USB-C RECEPTACLE + ESD", "3V3 LDO", "INPUT BUFFERS". Dashed stroke 0.254 mm, title inside the top-left corner.
- **Inside a block, wire parts pin-to-pin** with Manhattan wires. Put junctions at T-joins. Never lay wires on
  top of each other. Avoid crossings; one deliberate crossing is acceptable where the topology forces it.
  Example: a mux PR1 pin between two pins tied to the same rail.
- **Between blocks, use short net labels**: local within a sheet, hierarchical across sheets. Never use
  global labels, and never run long wires across the sheet.
- **No stub + label on every pin.** Labels are for leaving a block, or for a connector/IC pin whose net
  really goes elsewhere.
- **Rails and GND are power symbols at the part.** Named rails (VSYS, VBAT, VTREF, VBUS_x) are project
  power symbols: copies of `power:VAA` with the Value renamed and `(power global)`. Use stock `power:+3V3`
  and `power:GND` where they exist.
- **Drive enables straight from the controlling MCU signal** instead of looping them to VIN. An enable that
  must follow VIN (e.g. an LDO with no MCU before it) is wired to VIN on purpose.
- Put each block's notes **inside its box**: design intent, thresholds, firmware hooks.
- Add PWR_FLAGs in a small "POWER FLAGS (ERC)" block. Join each flag to its power symbol or label
  **with a wire**; a flag whose pin only overlaps another pin is reported as unconnected.
- Mark intentionally unused pins with a no-connect marker after checking datasheet requirements.

## Hierarchy and the root sheet

- Signals that leave a sub-sheet use a **hierarchical label** with a matching **sheet pin** on the parent,
  wired on the parent. The label shape (input/output/bidirectional/passive) should match the direction.
- Root sheet layout:
  - MCU sheet symbol in the centre, peripheral sheets either side.
  - Sheet pins aligned so each MCU connection is **one straight horizontal wire** between facing sheet pins.
  - MCU-sheet pins and their hierarchical labels are named by **bus instance or port pin**: `SPI00_SCK`,
    `P2.05`, `P1.04{slash}AIN0`, `PA9{slash}USART1_TX`, `PB3`. The other sheets use **functional** names
    (`FLASH_SCK`, `IMU_INT`, `SWCLK_OE`).
  - No stub labels at the root. Use a wire label only on a bus shared by three or more sheets.
  - Put a short system description (signal flow, key choices) in a note on the root.
- A design that fits in fewer sheets should use fewer. Put connectors on the sheet whose signals they carry
  rather than in a "connectors" sheet that needs pass-through pins everywhere.

## Pages, frame and title block

- **Embeint designs use A4 landscape.** Go to A3 only when a sheet's blocks don't fit. Set it with
  `SetPageSettings`.
- **Keep everything inside the frame:** at least 6 grid (15.24 mm) from the page edge. The default sheet
  has a 10 mm margin plus a 2 mm reference band. Stay clear of the title block (x >= W-120 mm, y >= H-44 mm).
  Use the actual page drawing-area/title-block bounds as the authority;
  `live_schematic_page` must report `fits: true`.
- **Fill the title block on every sheet** (`SetTitleBlockInfo`):
  - title `<project> - <sheet name>` (root: the project name);
  - date, revision;
  - company `Embeint Pty Ltd`;
  - comment1 = a one-line description, short enough to stay inside the title block;
  - comment2 = author (optional).

## Symbols on the sheet

- Symbol conventions: hidden `Manufacturer` and `MPN` fields; designator above value at the body's bottom
  right, left-aligned; datasheet-style pin groups (supplies first, then 7.62 mm before signals: VIN over EN,
  VOUT over ADJ); NC pins as hidden no-connects; stock body styling (device layer, 0.254 mm outline,
  body-colour fill); roomier than stock symbols.
- Values are plain, Embeint style: `0.1uF`, `4.7uF`, `10k`, `47R`. Ratings go in fields (Tolerance, Voltage
  Rating, Dielectric / Power Rating), not in the value.

## Layout mechanics that avoid overlaps

- Work on the 2.54 mm grid (1.27 mm where a pin needs it). Place symbol origins on grid so pins land on grid.
- Labels on adjacent pins are 1 grid apart; their text height (1.27 mm) needs that spacing. A power symbol's
  text sits about 1.4 grid above its pin, so keep it off neighbouring wires and labels.
- A rotated power symbol keeps its text horizontal. GND pointing left on a horizontal connector pin is fine.
- Keep symbol fields clear of neighbouring symbols. Series resistors on an output need about 7 grid of
  clearance from the IC's value text. Decoupling caps go beside the IC, not under its reference.
- **Always look at the rendered page** (`live_schematic_view`, frame on) after each sheet and fix every overlap
  before moving on.
