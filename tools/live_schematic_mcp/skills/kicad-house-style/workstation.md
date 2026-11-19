# Optional workstation configuration

This portable skill contains no private reference-design locations, client projects,
network shares or personal installation paths. Keep those in local configuration
outside the distributed skill; do not add them to published examples or evidence.

Resolve the active installation, project library tables and project path variables
before using libraries. `EMBEINT_PART_DIR`, when locally configured, can identify
the Embeint footprint/model library. Use project-relative `${KIPRJMOD}` references
where appropriate. Confirm paths in the editor's operating system: Windows paths
and WSL paths are not interchangeable. Do not rewrite global library tables as a
side effect of capturing a schematic.

If `kicad-part-selector` is available, use its configured checkout and its own Python
environment. Inspect its current API before invoking matchers; earlier versions use
`Requirements.from_fields`, `candidates_for`, `rejection_reasons` and
`selection_fields` in their capacitor/resistor modules. Apply the same rating and
manufacturer fields as the selector's Apply action. Configuration is optional;
absence of the selector does not prevent datasheet-based selection, but report
unverified sourcing rather than inventing candidates or complete MPN suffixes.

Use only reference designs authorized for this task and read them without modifying
them. Describe reusable conventions without publishing private circuit contents.
MCP registration, runtime selection and WSL setup belong to installation configuration,
not to electrical design intent. A new MCP process/session must initialize to receive
updated instructions and skill data; an existing session may retain earlier guidance.
