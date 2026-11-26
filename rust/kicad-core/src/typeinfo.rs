// SPDX-License-Identifier: GPL-3.0-or-later
//! `include/core/typeinfo.h` (unit `include.core.application.pure.p001`): the `KICAD_T`
//! class identification values, their classification predicates, and the `IsA` /
//! `dyn_cast` templates.
//!
//! `KICAD_T` is an unscoped enumeration without a fixed underlying type. Its enumerators
//! span -1 to 113, so its values are exactly those of a signed 8-bit bit-field
//! ([dcl.enum]/8): every integer in [-128, 127] is a valid `KICAD_T`, including the
//! non-enumerators, which take each switch's `default` branch. [`KicadT`] holds such a
//! value; converting an integer outside that range is refused, because the C++
//! conversion is undefined there ([expr.static.cast]/10).

/// A `KICAD_T` value: any integer in [-128, 127]. The associated constants are the
/// enumerators, in declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KicadT(i8);

/// An integer outside `KICAD_T`'s range of values; converting it is undefined in C++.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KicadTOutOfRange(pub i64);

impl KicadT {
    pub const NOT_USED: Self = Self(-1);
    pub const TYPE_NOT_INIT: Self = Self(0);
    pub const PCB_T: Self = Self(1);
    pub const SCREEN_T: Self = Self(2);
    pub const PCB_FOOTPRINT_T: Self = Self(3);
    pub const PCB_PAD_T: Self = Self(4);
    pub const PCB_SHAPE_T: Self = Self(5);
    pub const PCB_REFERENCE_IMAGE_T: Self = Self(6);
    pub const PCB_FIELD_T: Self = Self(7);
    pub const PCB_GENERATOR_T: Self = Self(8);
    pub const PCB_TEXT_T: Self = Self(9);
    pub const PCB_TEXTBOX_T: Self = Self(10);
    pub const PCB_TABLE_T: Self = Self(11);
    pub const PCB_TABLECELL_T: Self = Self(12);
    pub const PCB_TRACE_T: Self = Self(13);
    pub const PCB_VIA_T: Self = Self(14);
    pub const PCB_ARC_T: Self = Self(15);
    pub const PCB_MARKER_T: Self = Self(16);
    pub const PCB_DIMENSION_T: Self = Self(17);
    pub const PCB_BARCODE_T: Self = Self(18);
    pub const PCB_DIM_ALIGNED_T: Self = Self(19);
    pub const PCB_DIM_LEADER_T: Self = Self(20);
    pub const PCB_DIM_CENTER_T: Self = Self(21);
    pub const PCB_DIM_RADIAL_T: Self = Self(22);
    pub const PCB_DIM_ORTHOGONAL_T: Self = Self(23);
    pub const PCB_TARGET_T: Self = Self(24);
    pub const PCB_ZONE_T: Self = Self(25);
    pub const PCB_ITEM_LIST_T: Self = Self(26);
    pub const PCB_NETINFO_T: Self = Self(27);
    pub const PCB_GROUP_T: Self = Self(28);
    pub const PCB_BOARD_OUTLINE_T: Self = Self(29);
    pub const PCB_POINT_T: Self = Self(30);
    pub const PCB_FIELD_LOCATE_REFERENCE_T: Self = Self(31);
    pub const PCB_FIELD_LOCATE_VALUE_T: Self = Self(32);
    pub const PCB_FIELD_LOCATE_FOOTPRINT_T: Self = Self(33);
    pub const PCB_FIELD_LOCATE_DATASHEET_T: Self = Self(34);
    pub const PCB_LOCATE_STDVIA_T: Self = Self(35);
    pub const PCB_LOCATE_UVIA_T: Self = Self(36);
    pub const PCB_LOCATE_BLINDVIA_T: Self = Self(37);
    pub const PCB_LOCATE_BURIEDVIA_T: Self = Self(38);
    pub const PCB_LOCATE_TEXT_T: Self = Self(39);
    pub const PCB_LOCATE_HOLE_T: Self = Self(40);
    pub const PCB_LOCATE_PTH_T: Self = Self(41);
    pub const PCB_LOCATE_NPTH_T: Self = Self(42);
    pub const PCB_LOCATE_BOARD_EDGE_T: Self = Self(43);
    pub const PCB_SHAPE_LOCATE_SEGMENT_T: Self = Self(44);
    pub const PCB_SHAPE_LOCATE_RECT_T: Self = Self(45);
    pub const PCB_SHAPE_LOCATE_CIRCLE_T: Self = Self(46);
    pub const PCB_SHAPE_LOCATE_ARC_T: Self = Self(47);
    pub const PCB_SHAPE_LOCATE_POLY_T: Self = Self(48);
    pub const PCB_SHAPE_LOCATE_BEZIER_T: Self = Self(49);
    pub const LIB_SYMBOL_T: Self = Self(50);
    pub const SCH_SHAPE_T: Self = Self(51);
    pub const SCH_FIELD_T: Self = Self(52);
    pub const SCH_TEXT_T: Self = Self(53);
    pub const SCH_TEXTBOX_T: Self = Self(54);
    pub const SCH_PIN_T: Self = Self(55);
    pub const SCH_MARKER_T: Self = Self(56);
    pub const SCH_JUNCTION_T: Self = Self(57);
    pub const SCH_NO_CONNECT_T: Self = Self(58);
    pub const SCH_BUS_WIRE_ENTRY_T: Self = Self(59);
    pub const SCH_BUS_BUS_ENTRY_T: Self = Self(60);
    pub const SCH_LINE_T: Self = Self(61);
    pub const SCH_BITMAP_T: Self = Self(62);
    pub const SCH_TABLE_T: Self = Self(63);
    pub const SCH_TABLECELL_T: Self = Self(64);
    pub const SCH_LABEL_T: Self = Self(65);
    pub const SCH_GLOBAL_LABEL_T: Self = Self(66);
    pub const SCH_HIER_LABEL_T: Self = Self(67);
    pub const SCH_RULE_AREA_T: Self = Self(68);
    pub const SCH_DIRECTIVE_LABEL_T: Self = Self(69);
    pub const SCH_SYMBOL_T: Self = Self(70);
    pub const SCH_GROUP_T: Self = Self(71);
    pub const SCH_SHEET_PIN_T: Self = Self(72);
    pub const SCH_SHEET_T: Self = Self(73);
    pub const SCH_FIELD_LOCATE_REFERENCE_T: Self = Self(74);
    pub const SCH_FIELD_LOCATE_VALUE_T: Self = Self(75);
    pub const SCH_FIELD_LOCATE_FOOTPRINT_T: Self = Self(76);
    pub const SCH_FIELD_LOCATE_DATASHEET_T: Self = Self(77);
    pub const SCH_ITEM_LOCATE_WIRE_T: Self = Self(78);
    pub const SCH_ITEM_LOCATE_BUS_T: Self = Self(79);
    pub const SCH_ITEM_LOCATE_GRAPHIC_LINE_T: Self = Self(80);
    pub const SCH_LABEL_LOCATE_ANY_T: Self = Self(81);
    pub const SCH_LABEL_LOCATE_WIRE_T: Self = Self(82);
    pub const SCH_LABEL_LOCATE_BUS_T: Self = Self(83);
    pub const SCH_SYMBOL_LOCATE_POWER_T: Self = Self(84);
    pub const SCH_LOCATE_ANY_T: Self = Self(85);
    pub const SCH_SCREEN_T: Self = Self(86);
    pub const SCHEMATIC_T: Self = Self(87);
    pub const GERBER_LAYOUT_T: Self = Self(88);
    pub const GERBER_DRAW_ITEM_T: Self = Self(89);
    pub const GERBER_IMAGE_T: Self = Self(90);
    pub const WSG_LINE_T: Self = Self(91);
    pub const WSG_RECT_T: Self = Self(92);
    pub const WSG_POLY_T: Self = Self(93);
    pub const WSG_TEXT_T: Self = Self(94);
    pub const WSG_BITMAP_T: Self = Self(95);
    pub const WSG_PAGE_T: Self = Self(96);
    pub const WS_PROXY_UNDO_ITEM_T: Self = Self(97);
    pub const WS_PROXY_UNDO_ITEM_PLUS_T: Self = Self(98);
    pub const SYMBOL_LIB_TABLE_T: Self = Self(99);
    pub const FP_LIB_TABLE_T: Self = Self(100);
    pub const DESIGN_BLOCK_LIB_TABLE_T: Self = Self(101);
    pub const SYMBOL_LIBS_T: Self = Self(102);
    pub const SEARCH_STACK_T: Self = Self(103);
    pub const S3D_CACHE_T: Self = Self(104);
    pub const PCB_SHAPE_LOCATE_ELLIPSE_T: Self = Self(105);
    pub const PCB_SHAPE_LOCATE_ELLIPSE_ARC_T: Self = Self(106);
    pub const PCB_CONSTRAINT_T: Self = Self(107);
    pub const PCB_GRID_ITEM_T: Self = Self(108);
    pub const PCB_DRILL_CHART_T: Self = Self(109);
    pub const PCB_DRILL_MAP_T: Self = Self(110);
    pub const PCB_3D_BODY_T: Self = Self(111);
    pub const PCB_STACKUP_TABLE_T: Self = Self(112);
    pub const MAX_STRUCT_TYPE_ID: Self = Self(113);

    /// The value of an 8-bit integer; every one is a `KICAD_T`.
    pub const fn from_i8(value: i8) -> Self {
        Self(value)
    }

    /// `static_cast<KICAD_T>( aValue )`, refused where the C++ conversion is undefined.
    pub const fn new(value: i64) -> Result<Self, KicadTOutOfRange> {
        if value < i8::MIN as i64 || value > i8::MAX as i64 {
            return Err(KicadTOutOfRange(value));
        }
        Ok(Self(value as i8))
    }

    /// The integer value (the underlying `int` of the C++ enumeration).
    pub const fn value(self) -> i32 {
        self.0 as i32
    }
}

// static_assert( S3D_CACHE_T == 104, ... ): KICAD_T ordinals are exposed via IPC.
const _: () = assert!(
    KicadT::S3D_CACHE_T.value() == 104,
    "KICAD_T ordinals are exposed via IPC; new values must be appended at the end of the enum, \
     not inserted into existing blocks."
);

/// Every enumerator with its C++ name, in declaration order.
pub const ENUMERATORS: [(&str, KicadT); 115] = [
    ("NOT_USED", KicadT::NOT_USED),
    ("TYPE_NOT_INIT", KicadT::TYPE_NOT_INIT),
    ("PCB_T", KicadT::PCB_T),
    ("SCREEN_T", KicadT::SCREEN_T),
    ("PCB_FOOTPRINT_T", KicadT::PCB_FOOTPRINT_T),
    ("PCB_PAD_T", KicadT::PCB_PAD_T),
    ("PCB_SHAPE_T", KicadT::PCB_SHAPE_T),
    ("PCB_REFERENCE_IMAGE_T", KicadT::PCB_REFERENCE_IMAGE_T),
    ("PCB_FIELD_T", KicadT::PCB_FIELD_T),
    ("PCB_GENERATOR_T", KicadT::PCB_GENERATOR_T),
    ("PCB_TEXT_T", KicadT::PCB_TEXT_T),
    ("PCB_TEXTBOX_T", KicadT::PCB_TEXTBOX_T),
    ("PCB_TABLE_T", KicadT::PCB_TABLE_T),
    ("PCB_TABLECELL_T", KicadT::PCB_TABLECELL_T),
    ("PCB_TRACE_T", KicadT::PCB_TRACE_T),
    ("PCB_VIA_T", KicadT::PCB_VIA_T),
    ("PCB_ARC_T", KicadT::PCB_ARC_T),
    ("PCB_MARKER_T", KicadT::PCB_MARKER_T),
    ("PCB_DIMENSION_T", KicadT::PCB_DIMENSION_T),
    ("PCB_BARCODE_T", KicadT::PCB_BARCODE_T),
    ("PCB_DIM_ALIGNED_T", KicadT::PCB_DIM_ALIGNED_T),
    ("PCB_DIM_LEADER_T", KicadT::PCB_DIM_LEADER_T),
    ("PCB_DIM_CENTER_T", KicadT::PCB_DIM_CENTER_T),
    ("PCB_DIM_RADIAL_T", KicadT::PCB_DIM_RADIAL_T),
    ("PCB_DIM_ORTHOGONAL_T", KicadT::PCB_DIM_ORTHOGONAL_T),
    ("PCB_TARGET_T", KicadT::PCB_TARGET_T),
    ("PCB_ZONE_T", KicadT::PCB_ZONE_T),
    ("PCB_ITEM_LIST_T", KicadT::PCB_ITEM_LIST_T),
    ("PCB_NETINFO_T", KicadT::PCB_NETINFO_T),
    ("PCB_GROUP_T", KicadT::PCB_GROUP_T),
    ("PCB_BOARD_OUTLINE_T", KicadT::PCB_BOARD_OUTLINE_T),
    ("PCB_POINT_T", KicadT::PCB_POINT_T),
    (
        "PCB_FIELD_LOCATE_REFERENCE_T",
        KicadT::PCB_FIELD_LOCATE_REFERENCE_T,
    ),
    ("PCB_FIELD_LOCATE_VALUE_T", KicadT::PCB_FIELD_LOCATE_VALUE_T),
    (
        "PCB_FIELD_LOCATE_FOOTPRINT_T",
        KicadT::PCB_FIELD_LOCATE_FOOTPRINT_T,
    ),
    (
        "PCB_FIELD_LOCATE_DATASHEET_T",
        KicadT::PCB_FIELD_LOCATE_DATASHEET_T,
    ),
    ("PCB_LOCATE_STDVIA_T", KicadT::PCB_LOCATE_STDVIA_T),
    ("PCB_LOCATE_UVIA_T", KicadT::PCB_LOCATE_UVIA_T),
    ("PCB_LOCATE_BLINDVIA_T", KicadT::PCB_LOCATE_BLINDVIA_T),
    ("PCB_LOCATE_BURIEDVIA_T", KicadT::PCB_LOCATE_BURIEDVIA_T),
    ("PCB_LOCATE_TEXT_T", KicadT::PCB_LOCATE_TEXT_T),
    ("PCB_LOCATE_HOLE_T", KicadT::PCB_LOCATE_HOLE_T),
    ("PCB_LOCATE_PTH_T", KicadT::PCB_LOCATE_PTH_T),
    ("PCB_LOCATE_NPTH_T", KicadT::PCB_LOCATE_NPTH_T),
    ("PCB_LOCATE_BOARD_EDGE_T", KicadT::PCB_LOCATE_BOARD_EDGE_T),
    (
        "PCB_SHAPE_LOCATE_SEGMENT_T",
        KicadT::PCB_SHAPE_LOCATE_SEGMENT_T,
    ),
    ("PCB_SHAPE_LOCATE_RECT_T", KicadT::PCB_SHAPE_LOCATE_RECT_T),
    (
        "PCB_SHAPE_LOCATE_CIRCLE_T",
        KicadT::PCB_SHAPE_LOCATE_CIRCLE_T,
    ),
    ("PCB_SHAPE_LOCATE_ARC_T", KicadT::PCB_SHAPE_LOCATE_ARC_T),
    ("PCB_SHAPE_LOCATE_POLY_T", KicadT::PCB_SHAPE_LOCATE_POLY_T),
    (
        "PCB_SHAPE_LOCATE_BEZIER_T",
        KicadT::PCB_SHAPE_LOCATE_BEZIER_T,
    ),
    ("LIB_SYMBOL_T", KicadT::LIB_SYMBOL_T),
    ("SCH_SHAPE_T", KicadT::SCH_SHAPE_T),
    ("SCH_FIELD_T", KicadT::SCH_FIELD_T),
    ("SCH_TEXT_T", KicadT::SCH_TEXT_T),
    ("SCH_TEXTBOX_T", KicadT::SCH_TEXTBOX_T),
    ("SCH_PIN_T", KicadT::SCH_PIN_T),
    ("SCH_MARKER_T", KicadT::SCH_MARKER_T),
    ("SCH_JUNCTION_T", KicadT::SCH_JUNCTION_T),
    ("SCH_NO_CONNECT_T", KicadT::SCH_NO_CONNECT_T),
    ("SCH_BUS_WIRE_ENTRY_T", KicadT::SCH_BUS_WIRE_ENTRY_T),
    ("SCH_BUS_BUS_ENTRY_T", KicadT::SCH_BUS_BUS_ENTRY_T),
    ("SCH_LINE_T", KicadT::SCH_LINE_T),
    ("SCH_BITMAP_T", KicadT::SCH_BITMAP_T),
    ("SCH_TABLE_T", KicadT::SCH_TABLE_T),
    ("SCH_TABLECELL_T", KicadT::SCH_TABLECELL_T),
    ("SCH_LABEL_T", KicadT::SCH_LABEL_T),
    ("SCH_GLOBAL_LABEL_T", KicadT::SCH_GLOBAL_LABEL_T),
    ("SCH_HIER_LABEL_T", KicadT::SCH_HIER_LABEL_T),
    ("SCH_RULE_AREA_T", KicadT::SCH_RULE_AREA_T),
    ("SCH_DIRECTIVE_LABEL_T", KicadT::SCH_DIRECTIVE_LABEL_T),
    ("SCH_SYMBOL_T", KicadT::SCH_SYMBOL_T),
    ("SCH_GROUP_T", KicadT::SCH_GROUP_T),
    ("SCH_SHEET_PIN_T", KicadT::SCH_SHEET_PIN_T),
    ("SCH_SHEET_T", KicadT::SCH_SHEET_T),
    (
        "SCH_FIELD_LOCATE_REFERENCE_T",
        KicadT::SCH_FIELD_LOCATE_REFERENCE_T,
    ),
    ("SCH_FIELD_LOCATE_VALUE_T", KicadT::SCH_FIELD_LOCATE_VALUE_T),
    (
        "SCH_FIELD_LOCATE_FOOTPRINT_T",
        KicadT::SCH_FIELD_LOCATE_FOOTPRINT_T,
    ),
    (
        "SCH_FIELD_LOCATE_DATASHEET_T",
        KicadT::SCH_FIELD_LOCATE_DATASHEET_T,
    ),
    ("SCH_ITEM_LOCATE_WIRE_T", KicadT::SCH_ITEM_LOCATE_WIRE_T),
    ("SCH_ITEM_LOCATE_BUS_T", KicadT::SCH_ITEM_LOCATE_BUS_T),
    (
        "SCH_ITEM_LOCATE_GRAPHIC_LINE_T",
        KicadT::SCH_ITEM_LOCATE_GRAPHIC_LINE_T,
    ),
    ("SCH_LABEL_LOCATE_ANY_T", KicadT::SCH_LABEL_LOCATE_ANY_T),
    ("SCH_LABEL_LOCATE_WIRE_T", KicadT::SCH_LABEL_LOCATE_WIRE_T),
    ("SCH_LABEL_LOCATE_BUS_T", KicadT::SCH_LABEL_LOCATE_BUS_T),
    (
        "SCH_SYMBOL_LOCATE_POWER_T",
        KicadT::SCH_SYMBOL_LOCATE_POWER_T,
    ),
    ("SCH_LOCATE_ANY_T", KicadT::SCH_LOCATE_ANY_T),
    ("SCH_SCREEN_T", KicadT::SCH_SCREEN_T),
    ("SCHEMATIC_T", KicadT::SCHEMATIC_T),
    ("GERBER_LAYOUT_T", KicadT::GERBER_LAYOUT_T),
    ("GERBER_DRAW_ITEM_T", KicadT::GERBER_DRAW_ITEM_T),
    ("GERBER_IMAGE_T", KicadT::GERBER_IMAGE_T),
    ("WSG_LINE_T", KicadT::WSG_LINE_T),
    ("WSG_RECT_T", KicadT::WSG_RECT_T),
    ("WSG_POLY_T", KicadT::WSG_POLY_T),
    ("WSG_TEXT_T", KicadT::WSG_TEXT_T),
    ("WSG_BITMAP_T", KicadT::WSG_BITMAP_T),
    ("WSG_PAGE_T", KicadT::WSG_PAGE_T),
    ("WS_PROXY_UNDO_ITEM_T", KicadT::WS_PROXY_UNDO_ITEM_T),
    (
        "WS_PROXY_UNDO_ITEM_PLUS_T",
        KicadT::WS_PROXY_UNDO_ITEM_PLUS_T,
    ),
    ("SYMBOL_LIB_TABLE_T", KicadT::SYMBOL_LIB_TABLE_T),
    ("FP_LIB_TABLE_T", KicadT::FP_LIB_TABLE_T),
    ("DESIGN_BLOCK_LIB_TABLE_T", KicadT::DESIGN_BLOCK_LIB_TABLE_T),
    ("SYMBOL_LIBS_T", KicadT::SYMBOL_LIBS_T),
    ("SEARCH_STACK_T", KicadT::SEARCH_STACK_T),
    ("S3D_CACHE_T", KicadT::S3D_CACHE_T),
    (
        "PCB_SHAPE_LOCATE_ELLIPSE_T",
        KicadT::PCB_SHAPE_LOCATE_ELLIPSE_T,
    ),
    (
        "PCB_SHAPE_LOCATE_ELLIPSE_ARC_T",
        KicadT::PCB_SHAPE_LOCATE_ELLIPSE_ARC_T,
    ),
    ("PCB_CONSTRAINT_T", KicadT::PCB_CONSTRAINT_T),
    ("PCB_GRID_ITEM_T", KicadT::PCB_GRID_ITEM_T),
    ("PCB_DRILL_CHART_T", KicadT::PCB_DRILL_CHART_T),
    ("PCB_DRILL_MAP_T", KicadT::PCB_DRILL_MAP_T),
    ("PCB_3D_BODY_T", KicadT::PCB_3D_BODY_T),
    ("PCB_STACKUP_TABLE_T", KicadT::PCB_STACKUP_TABLE_T),
    ("MAX_STRUCT_TYPE_ID", KicadT::MAX_STRUCT_TYPE_ID),
];

/// `BaseType( aType )`: the underlying type of one of the "locate" pseudo-types.
pub const fn base_type(a_type: KicadT) -> KicadT {
    match a_type {
        KicadT::SCH_FIELD_LOCATE_REFERENCE_T
        | KicadT::SCH_FIELD_LOCATE_VALUE_T
        | KicadT::SCH_FIELD_LOCATE_FOOTPRINT_T
        | KicadT::SCH_FIELD_LOCATE_DATASHEET_T => KicadT::SCH_FIELD_T,

        KicadT::SCH_ITEM_LOCATE_WIRE_T
        | KicadT::SCH_ITEM_LOCATE_BUS_T
        | KicadT::SCH_ITEM_LOCATE_GRAPHIC_LINE_T => KicadT::SCH_LINE_T,

        KicadT::SCH_LABEL_LOCATE_ANY_T
        | KicadT::SCH_LABEL_LOCATE_WIRE_T
        | KicadT::SCH_LABEL_LOCATE_BUS_T => KicadT::SCH_LABEL_T,

        KicadT::SCH_SYMBOL_LOCATE_POWER_T => KicadT::SCH_SYMBOL_T,

        KicadT::PCB_FIELD_LOCATE_REFERENCE_T
        | KicadT::PCB_FIELD_LOCATE_VALUE_T
        | KicadT::PCB_FIELD_LOCATE_FOOTPRINT_T
        | KicadT::PCB_FIELD_LOCATE_DATASHEET_T => KicadT::PCB_FIELD_T,

        KicadT::PCB_LOCATE_HOLE_T | KicadT::PCB_LOCATE_PTH_T | KicadT::PCB_LOCATE_NPTH_T => {
            KicadT::PCB_LOCATE_HOLE_T
        }

        KicadT::PCB_SHAPE_LOCATE_SEGMENT_T
        | KicadT::PCB_SHAPE_LOCATE_RECT_T
        | KicadT::PCB_SHAPE_LOCATE_CIRCLE_T
        | KicadT::PCB_SHAPE_LOCATE_ARC_T
        | KicadT::PCB_SHAPE_LOCATE_POLY_T
        | KicadT::PCB_SHAPE_LOCATE_BEZIER_T
        | KicadT::PCB_SHAPE_LOCATE_ELLIPSE_T
        | KicadT::PCB_SHAPE_LOCATE_ELLIPSE_ARC_T => KicadT::PCB_SHAPE_T,

        KicadT::PCB_DIM_ALIGNED_T
        | KicadT::PCB_DIM_CENTER_T
        | KicadT::PCB_DIM_RADIAL_T
        | KicadT::PCB_DIM_ORTHOGONAL_T
        | KicadT::PCB_DIM_LEADER_T => KicadT::PCB_DIMENSION_T,

        KicadT::PCB_DRILL_CHART_T | KicadT::PCB_STACKUP_TABLE_T => KicadT::PCB_TABLE_T,

        _ => a_type,
    }
}

/// `IsNullType( aType )`: `aType <= 0`.
pub const fn is_null_type(a_type: KicadT) -> bool {
    a_type.value() <= 0
}

/// `IsGeneratedTableType( aType )`: a `PCB_TABLE` subclass whose cells come from the board.
pub const fn is_generated_table_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::PCB_DRILL_CHART_T | KicadT::PCB_STACKUP_TABLE_T
    )
}

/// `IsInstantiableType( aType )`.
pub const fn is_instantiable_type(a_type: KicadT) -> bool {
    if is_null_type(a_type) {
        return false;
    }

    match a_type {
        KicadT::SCH_LOCATE_ANY_T
        | KicadT::SCH_FIELD_LOCATE_REFERENCE_T
        | KicadT::SCH_FIELD_LOCATE_VALUE_T
        | KicadT::SCH_FIELD_LOCATE_FOOTPRINT_T
        | KicadT::SCH_FIELD_LOCATE_DATASHEET_T
        | KicadT::SCH_ITEM_LOCATE_WIRE_T
        | KicadT::SCH_ITEM_LOCATE_BUS_T
        | KicadT::SCH_ITEM_LOCATE_GRAPHIC_LINE_T
        | KicadT::SCH_LABEL_LOCATE_ANY_T
        | KicadT::SCH_LABEL_LOCATE_WIRE_T
        | KicadT::SCH_LABEL_LOCATE_BUS_T
        | KicadT::SCH_SYMBOL_LOCATE_POWER_T
        | KicadT::PCB_FIELD_LOCATE_REFERENCE_T
        | KicadT::PCB_FIELD_LOCATE_VALUE_T
        | KicadT::PCB_FIELD_LOCATE_FOOTPRINT_T
        | KicadT::PCB_FIELD_LOCATE_DATASHEET_T
        | KicadT::PCB_LOCATE_STDVIA_T
        | KicadT::PCB_LOCATE_UVIA_T
        | KicadT::PCB_LOCATE_BLINDVIA_T
        | KicadT::PCB_LOCATE_BURIEDVIA_T
        | KicadT::PCB_LOCATE_TEXT_T
        | KicadT::PCB_LOCATE_HOLE_T
        | KicadT::PCB_LOCATE_PTH_T
        | KicadT::PCB_LOCATE_NPTH_T
        | KicadT::PCB_LOCATE_BOARD_EDGE_T
        | KicadT::PCB_SHAPE_LOCATE_SEGMENT_T
        | KicadT::PCB_SHAPE_LOCATE_RECT_T
        | KicadT::PCB_SHAPE_LOCATE_CIRCLE_T
        | KicadT::PCB_SHAPE_LOCATE_ARC_T
        | KicadT::PCB_SHAPE_LOCATE_POLY_T
        | KicadT::PCB_SHAPE_LOCATE_BEZIER_T
        | KicadT::PCB_SHAPE_LOCATE_ELLIPSE_T
        | KicadT::PCB_SHAPE_LOCATE_ELLIPSE_ARC_T => false,

        // base class is not instantiable
        KicadT::PCB_DIMENSION_T => false,

        // must be instantiated as part of a SCH_TABLE or PCB_TABLE
        KicadT::SCH_TABLECELL_T | KicadT::PCB_TABLECELL_T => false,

        KicadT::SCH_SCREEN_T | KicadT::PCB_ITEM_LIST_T => false,

        _ => true,
    }
}

/// `IsEeschemaType( aType )`.
pub const fn is_eeschema_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::SCH_MARKER_T
            | KicadT::SCH_JUNCTION_T
            | KicadT::SCH_NO_CONNECT_T
            | KicadT::SCH_BUS_WIRE_ENTRY_T
            | KicadT::SCH_BUS_BUS_ENTRY_T
            | KicadT::SCH_LINE_T
            | KicadT::SCH_SHAPE_T
            | KicadT::SCH_RULE_AREA_T
            | KicadT::SCH_BITMAP_T
            | KicadT::SCH_TEXT_T
            | KicadT::SCH_TEXTBOX_T
            | KicadT::SCH_TABLE_T
            | KicadT::SCH_TABLECELL_T
            | KicadT::SCH_LABEL_T
            | KicadT::SCH_DIRECTIVE_LABEL_T
            | KicadT::SCH_GLOBAL_LABEL_T
            | KicadT::SCH_HIER_LABEL_T
            | KicadT::SCH_FIELD_T
            | KicadT::SCH_SYMBOL_T
            | KicadT::SCH_SHEET_PIN_T
            | KicadT::SCH_GROUP_T
            | KicadT::SCH_SHEET_T
            | KicadT::SCH_PIN_T
            | KicadT::SCH_FIELD_LOCATE_REFERENCE_T
            | KicadT::SCH_FIELD_LOCATE_VALUE_T
            | KicadT::SCH_FIELD_LOCATE_FOOTPRINT_T
            | KicadT::SCH_FIELD_LOCATE_DATASHEET_T
            | KicadT::SCH_ITEM_LOCATE_WIRE_T
            | KicadT::SCH_ITEM_LOCATE_BUS_T
            | KicadT::SCH_ITEM_LOCATE_GRAPHIC_LINE_T
            | KicadT::SCH_LABEL_LOCATE_ANY_T
            | KicadT::SCH_LABEL_LOCATE_WIRE_T
            | KicadT::SCH_LABEL_LOCATE_BUS_T
            | KicadT::SCH_SYMBOL_LOCATE_POWER_T
            | KicadT::SCH_LOCATE_ANY_T
            | KicadT::SCH_SCREEN_T
            | KicadT::SCHEMATIC_T
            | KicadT::LIB_SYMBOL_T
    )
}

/// `IsPcbnewType( aType )`.
pub const fn is_pcbnew_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::PCB_T
            | KicadT::PCB_FOOTPRINT_T
            | KicadT::PCB_PAD_T
            | KicadT::PCB_SHAPE_T
            | KicadT::PCB_REFERENCE_IMAGE_T
            | KicadT::PCB_FIELD_T
            | KicadT::PCB_TEXT_T
            | KicadT::PCB_TEXTBOX_T
            | KicadT::PCB_BARCODE_T
            | KicadT::PCB_TABLE_T
            | KicadT::PCB_TABLECELL_T
            | KicadT::PCB_TRACE_T
            | KicadT::PCB_VIA_T
            | KicadT::PCB_ARC_T
            | KicadT::PCB_MARKER_T
            | KicadT::PCB_DIMENSION_T
            | KicadT::PCB_DIM_ALIGNED_T
            | KicadT::PCB_DIM_LEADER_T
            | KicadT::PCB_DIM_CENTER_T
            | KicadT::PCB_DIM_RADIAL_T
            | KicadT::PCB_DIM_ORTHOGONAL_T
            | KicadT::PCB_TARGET_T
            | KicadT::PCB_DRILL_CHART_T
            | KicadT::PCB_DRILL_MAP_T
            | KicadT::PCB_3D_BODY_T
            | KicadT::PCB_STACKUP_TABLE_T
            | KicadT::PCB_POINT_T
            | KicadT::PCB_ZONE_T
            | KicadT::PCB_ITEM_LIST_T
            | KicadT::PCB_NETINFO_T
            | KicadT::PCB_GROUP_T
            | KicadT::PCB_GENERATOR_T
            | KicadT::PCB_GRID_ITEM_T
            | KicadT::PCB_FIELD_LOCATE_REFERENCE_T
            | KicadT::PCB_FIELD_LOCATE_VALUE_T
            | KicadT::PCB_FIELD_LOCATE_FOOTPRINT_T
            | KicadT::PCB_FIELD_LOCATE_DATASHEET_T
            | KicadT::PCB_LOCATE_STDVIA_T
            | KicadT::PCB_LOCATE_UVIA_T
            | KicadT::PCB_LOCATE_BLINDVIA_T
            | KicadT::PCB_LOCATE_BURIEDVIA_T
            | KicadT::PCB_LOCATE_TEXT_T
            | KicadT::PCB_LOCATE_HOLE_T
            | KicadT::PCB_LOCATE_PTH_T
            | KicadT::PCB_LOCATE_NPTH_T
            | KicadT::PCB_LOCATE_BOARD_EDGE_T
            | KicadT::PCB_SHAPE_LOCATE_SEGMENT_T
            | KicadT::PCB_SHAPE_LOCATE_RECT_T
            | KicadT::PCB_SHAPE_LOCATE_CIRCLE_T
            | KicadT::PCB_SHAPE_LOCATE_ARC_T
            | KicadT::PCB_SHAPE_LOCATE_POLY_T
            | KicadT::PCB_SHAPE_LOCATE_BEZIER_T
            | KicadT::PCB_SHAPE_LOCATE_ELLIPSE_T
            | KicadT::PCB_SHAPE_LOCATE_ELLIPSE_ARC_T
            | KicadT::PCB_BOARD_OUTLINE_T
            | KicadT::PCB_CONSTRAINT_T
    )
}

/// `IsSingleLayerType( aType )`.
pub const fn is_single_layer_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::PCB_SHAPE_T
            | KicadT::PCB_REFERENCE_IMAGE_T
            | KicadT::PCB_FIELD_T
            | KicadT::PCB_TEXT_T
            | KicadT::PCB_TEXTBOX_T
            | KicadT::PCB_BARCODE_T
            | KicadT::PCB_TABLE_T
            | KicadT::PCB_DRILL_CHART_T
            | KicadT::PCB_DRILL_MAP_T
            | KicadT::PCB_STACKUP_TABLE_T
            | KicadT::PCB_TABLECELL_T
            | KicadT::PCB_TRACE_T
            | KicadT::PCB_ARC_T
            | KicadT::PCB_MARKER_T
            | KicadT::PCB_DIMENSION_T
            | KicadT::PCB_DIM_ALIGNED_T
            | KicadT::PCB_DIM_LEADER_T
            | KicadT::PCB_DIM_CENTER_T
            | KicadT::PCB_DIM_RADIAL_T
            | KicadT::PCB_DIM_ORTHOGONAL_T
            | KicadT::PCB_TARGET_T
            | KicadT::PCB_POINT_T
            | KicadT::PCB_GENERATOR_T
    )
}

/// `IsGerbviewType( aType )`.
pub const fn is_gerbview_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::GERBER_LAYOUT_T | KicadT::GERBER_DRAW_ITEM_T | KicadT::GERBER_IMAGE_T
    )
}

/// `IsPageLayoutEditorType( aType )`.
pub const fn is_page_layout_editor_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::WSG_LINE_T
            | KicadT::WSG_RECT_T
            | KicadT::WSG_POLY_T
            | KicadT::WSG_TEXT_T
            | KicadT::WSG_BITMAP_T
            | KicadT::WSG_PAGE_T
            | KicadT::WS_PROXY_UNDO_ITEM_T
            | KicadT::WS_PROXY_UNDO_ITEM_PLUS_T
    )
}

/// `IsMiscType( aType )`.
pub const fn is_misc_type(a_type: KicadT) -> bool {
    matches!(
        a_type,
        KicadT::SCREEN_T
            | KicadT::SYMBOL_LIB_TABLE_T
            | KicadT::FP_LIB_TABLE_T
            | KicadT::DESIGN_BLOCK_LIB_TABLE_T
            | KicadT::SYMBOL_LIBS_T
            | KicadT::SEARCH_STACK_T
            | KicadT::S3D_CACHE_T
    )
}

/// `IsTypeCorrect( aType )`, with the same short-circuit order.
pub const fn is_type_correct(a_type: KicadT) -> bool {
    is_null_type(a_type)
        || is_eeschema_type(a_type)
        || is_pcbnew_type(a_type)
        || is_gerbview_type(a_type)
        || is_page_layout_editor_type(a_type)
        || is_misc_type(a_type)
}

/// A class with the static `ClassOf( const I* )` that `IsA` and `dyn_cast` call. Every real
/// implementation (the `EDA_ITEM` subclasses, `PNS::ITEM` subclasses) belongs to the unit
/// that owns the class; none is ported yet.
pub trait ClassOf<I: ?Sized> {
    /// `T::ClassOf( aObject )`; `None` is a null pointer.
    fn class_of(object: Option<&I>) -> bool;

    /// `static_cast<T*>( aObject )` after `ClassOf` accepted the object. C++ cannot fail
    /// here; an implementation returns `None` only if it disagrees with its own `class_of`.
    fn static_cast(object: &I) -> Option<&Self>;
}

/// `IsA<T>( const I* aObject )`: `aObject && T::ClassOf( aObject )`. A null pointer never
/// reaches `ClassOf`.
pub fn is_a<T: ClassOf<I> + ?Sized, I: ?Sized>(object: Option<&I>) -> bool {
    object.is_some() && T::class_of(object)
}

/// `IsA<T>( const I& aObject )`: `T::ClassOf( &aObject )`.
pub fn is_a_ref<T: ClassOf<I> + ?Sized, I: ?Sized>(object: &I) -> bool {
    T::class_of(Some(object))
}

/// `dyn_cast<Casted*>( aObject )`: the object as `T` when `T::ClassOf` accepts it (a null
/// pointer included, which every real `ClassOf` rejects), otherwise `nullptr`.
pub fn dyn_cast<T: ClassOf<I> + ?Sized, I: ?Sized>(object: Option<&I>) -> Option<&T> {
    if T::class_of(object) {
        return object.and_then(T::static_cast);
    }

    None
}
