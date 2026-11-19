# Parts, symbols, footprints and fields

## Supplier rules

- **No Analog Devices parts**, including the Linear Technology and Maxim lines (Karl, 2026-11-17: poor value,
  expensive, supply-limited). Any other maker is fine: TI, ST, Microchip, onsemi, Nexperia, Diodes, Renesas,
  Rohm and others. Flag any ADI part that shows up in a review.
- Choose on value and multi-sourcing, not brand.

## Passives: 0402, multi-sourced, picked with kicad-part-selector

- **Default 0402.** Go up to 0603/0805/1206 where required ratings or verified sourcing aren't
  multi-sourced in 0402. Being orderable is not enough: **no capacitor or resistor may depend on a single
  manufacturer.** These are small production boards, and a single-source passive is a supply risk.
- **Use kicad-part-selector when available** (optional local configuration in
  [workstation.md](workstation.md)). Validate current matcher outputs against requirements.
  If unavailable, use manufacturer datasheets and verified sourcing evidence. A packaging
  suffix marked unverified is a catalogue reference, not a complete orderable MPN.
- **Ranking:**
  - MLCC: Samsung > KEMET; Murata only as an alternate (its `#` codes need the packaging suffix).
  - Resistors: Panasonic ERJ > Vishay CRCW > Stackpole.
  - Never NRND parts; tolerance as specified; voltage and power ratings that meet
    worst-case stress with appropriate derating.
- **Fields on every passive:**
  - Value (plain: `0.1uF`, `10k`, `47R`), Footprint, Datasheet (the maker's public URL, not the bundled local PDF path).
  - Hidden Manufacturer, MPN, Alt Manufacturer, Alt MPN.
  - Tolerance, Voltage Rating, Dielectric (caps) or Power Rating (resistors), Sources (the makers that meet the spec).
- **Requirement values must be realistic.** Resistor voltage is limited by both rated working
  voltage and sqrt(P*R). Derive stress from actual circuit conditions and transients rather
  than imposing a blanket voltage requirement that eliminates suitable low-value parts.
- Defaults: 1 % thick film unless precision is really needed; X7R/X5R MLCC; C0G for timing or filters.
  Check effective capacitance under DC bias for bulk caps.

## Embeint starting points (verify for the current design)

- **SWD programming:** Tag-Connect TC2030-NL (`Connector:Conn_ARM_SWD_TagConnect_TC2030-NL`, footprint
  `Connector:Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical`). It is pads only, so mark it not in BOM.
- **32.768 kHz crystal:** TXC 9HT11-32.768KDZC-T on Crystal_SMD_2012.
- **Flash:** Winbond W25Q128JW (1.8 V) or W25Q128JVPIQ (3 V), WSON-8 6x5.
- **IMU / sensors:** ST LSM6DSVTR (Embeint_LGA:14-VFLGA_2.5x3.0mm) with an IIS2MDCTR magnetometer on the IMU
  sensor hub; LPS22HH pressure; SHT41-AD1F humidity.
- **Load switches:** TI TPS22917.
- **USB ESD:** ST USBLC6-2SC6.
- **Castellated module edges:** `Embeint_Connectors:Castellated_Pin_<n>Pos_P2.54mm_2mmx2mm_1.5mm`.
- Reuse authorized reference circuits where suitable; revalidate exact part, package, ratings,
  footprint and availability. These starting points are not automatic substitutions.

## Project "atomic parts" library (recommended)

Give each project its own symbol library (e.g. `<project>.kicad_sym`, registered in the project
`sym-lib-table` as `${KIPRJMOD}/...`). Each entry is a copy of a stock or Embeint symbol with everything baked in:

- Name it by part and value: `R_10k_0402`, `C_0.1uF_0402`, `STM32C071KBU6`, `SN74LV1T126DCKR`.
- Set Value, Footprint, Datasheet, Description, Manufacturer, MPN, Alt Manufacturer and Alt MPN, plus the
  selector rating fields.
- Mark board features (castellations, fiducials, solder jumpers, Tag-Connect pads) `in_bom no`.
- Add project power symbols for named rails (copies of `power:VAA`, Value = rail name).
- Resolve unspecified pin types from the datasheet and actual function. Preserve power, input,
  output and open-drain semantics; do not blanket-convert pins to passive to silence ERC.
- **Flow-through parts** (USBLC6 ESD, some TVS arrays) have two pins per line. Declare them with
  jumper groups in the library symbol only when the datasheet confirms internally connected
  pads (e.g. groups "1"/"6" and "3"/"4" for the matching USBLC6 variant). Verify the exact
  symbol syntax, exported net membership and required flow-through PCB geometry.

Complete library fields reduce per-instance edits and make the BOM reviewable at placement.
Historical UpdateItems quirks are unverified on current builds (see [live-workflow.md](live-workflow.md)).
After regenerating the library file, check current schemas before sending
`ReloadLibrary` (symbol, project scope, the nickname) and `UpdateSymbolsFromLibrary` for the changed instances.

## Library conventions (new symbols and footprints)

- **Symbols:**
  - Hidden `Manufacturer` and `MPN` fields.
  - Designator above the value at the body's bottom right, left-aligned.
  - Datasheet-style pin groups.
  - NC pins as hidden no-connects.
  - Stock body styling.
  - Roomier than stock symbols.
- **Footprints:** KiCad Library Conventions.
  - Pads from the datasheet land pattern.
  - 0.12 mm silk kept 0.2 mm clear of copper.
  - 0.10 mm fab outline with a chamfered pin 1 and `${REFERENCE}`.
  - 0.05 mm courtyard 0.25 mm out, on a 0.01 mm grid.
- **Datasheets are linked** in the Datasheet field.
- **STEP models are embedded** in the footprint (`kicad-embed://`). SolidCraft builds missing models
  (`live_footprint_model_plan`).
- Check that a stock footprint exists before choosing a package. Read the footprint files from disk when the
  editor is busy: they're in the install's `share/footprints`.
