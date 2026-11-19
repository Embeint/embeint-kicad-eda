# Electrical and firmware review

## Pin planning

Before capture, record signal, physical pin, port, peripheral instance, direction,
voltage domain, reset state and firmware binding. Cross-check the exact package's
datasheet pin table against the actual library symbol and footprint. Check pinctrl,
HAL and driver source for the firmware revision in use; record source/version and
section or line, and mark unresolved assumptions.

Prefer separate flash and IMU SPI buses in Embeint designs. Keep related bus pins
and chip select on suitable ports where the MCU permits it. Validate peripheral
mapping, clock-capable pins, wake/interrupt capability and reserved debug, crystal,
NFC and boot pins for the chosen device. These properties are not portable between
MCU families or packages.

Earlier nRF54L15 planning distinguished P2 MCU, P1 peripheral and P0 low-power
domains, fixed SPIM00 pins, analog inputs, NFC/crystal and trace conflicts. Treat
those as device-specific review topics, not a reusable pin assignment table:
revalidate against the current Nordic documentation, module pinout and firmware.
Likewise, a Zephyr SWD GPIO driver's direction/OE polarity can depend on its fast
path and revision; inspect the implementation before selecting buffer polarity.

## Power and interfaces

- Define source selection and reverse-current behavior for every multi-source
  input. Choose a priority mux where appropriate and align data switching with
  power selection. Calculate thresholds and enable behavior from actual datasheets.
- For interfaces to another board's rail, check both-on, either-off and both-off
  states. Use suitable Ioff/Hi-Z behavior and disabled-state OE pulls. Avoid pull-ups
  or ESD paths that phantom-power an unpowered rail.
- Target debug reset should use open-drain drive with a safe default. Do not power
  a target by default; provide the intended jumper or controlled switch.
- For a USB-C sink, check separate CC Rd resistors, ESD placement, VBUS capacitance
  and inrush against the applicable USB requirements. Verify MCU USB supply/clock
  constraints and firmware support; do not generalize one MCU's supply range or
  one device's capacitance limit to every USB topology.
- Set electrical pin types from verified functions and ratings. Power flags indicate
  real sources and no-connects indicate genuinely unused pins; neither substitutes
  for checking the circuit.

Report what was established from datasheets, firmware and netlists, what still
needs firmware work, and what needs bench validation (timing, enumeration, power
sequencing, thresholds). A review or clean ERC does not establish measured behavior.
