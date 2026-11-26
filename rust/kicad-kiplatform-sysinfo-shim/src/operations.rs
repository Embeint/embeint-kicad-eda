// SPDX-License-Identifier: GPL-3.0-or-later
//! Operation decoding for the shim. Validation order and messages match the C++
//! adapter exactly; only validation, conversion and calls into
//! `kicad-kiplatform-sysinfo` live here.

use kicad_kiplatform_sysinfo::drivers;
use kicad_kiplatform_sysinfo::sysinfo::{CpuInfo, GpuInfo, MemoryInfo, Sysinfo};
use serde_json::{json, Map, Value};

/// Largest number of CPU or GPU entries in one request.
pub const MAX_ENTRIES: usize = 64;
/// Largest number of lifecycle steps in one request.
pub const MAX_STEPS: usize = 64;
/// Largest byte length of one string field.
pub const MAX_TEXT_BYTES: usize = 4096;

pub type Result<T> = core::result::Result<T, String>;

fn check_keys(input: &Value, fields: &[&str], place: &str) -> Result<()> {
    let ok = input.as_object().is_some_and(|object| {
        object.len() == fields.len() && fields.iter().all(|f| object.contains_key(*f))
    });
    if ok {
        Ok(())
    } else if place.is_empty() {
        Err("unexpected or missing field".into())
    } else {
        Err(format!("unexpected or missing field: {place}"))
    }
}

fn member<'a>(input: &'a Value, name: &str) -> Result<&'a Value> {
    input
        .get(name)
        .ok_or_else(|| "unexpected or missing field".to_owned())
}

/// Unsigned field in `[0, max]`; like nlohmann, `-0` is a signed zero and accepted.
fn unsigned(value: &Value, name: &str, max: u64) -> Result<u64> {
    let refused = || format!("integer out of range: {name}");
    if let Some(number) = value.as_u64() {
        return if number <= max {
            Ok(number)
        } else {
            Err(refused())
        };
    }
    match value.as_i64() {
        Some(number) => u64::try_from(number)
            .ok()
            .filter(|n| *n <= max)
            .ok_or_else(refused),
        None => Err(refused()),
    }
}

fn unsigned32(value: &Value, name: &str) -> Result<u32> {
    let number = unsigned(value, name, u64::from(u32::MAX))?;
    u32::try_from(number).map_err(|_| format!("integer out of range: {name}"))
}

fn signed(value: &Value, name: &str) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| format!("integer out of range: {name}"))
}

fn digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// `std::string` bytes travel as lower-case hex so NUL and non-UTF-8 bytes are exact.
fn bytes(value: &Value, name: &str) -> Result<Vec<u8>> {
    let text = value
        .as_str()
        .ok_or_else(|| format!("expected string: {name}"))?
        .as_bytes();
    let invalid = || format!("invalid hex: {name}");
    if text.len() % 2 != 0
        || text.len() > 2 * MAX_TEXT_BYTES
        || text.iter().any(|c| digit(*c).is_none())
    {
        return Err(invalid());
    }
    text.chunks_exact(2)
        .map(|pair| match pair {
            [high, low] => Some((digit(*high)? << 4) | digit(*low)?),
            _ => None,
        })
        .collect::<Option<Vec<u8>>>()
        .ok_or_else(invalid)
}

fn array<'a>(input: &'a Value, name: &str, max: usize) -> Result<&'a Vec<Value>> {
    let value = member(input, name)?
        .as_array()
        .ok_or_else(|| format!("expected array: {name}"))?;
    if value.len() > max {
        return Err(format!("too many entries: {name}"));
    }
    Ok(value)
}

fn hex(data: &[u8]) -> Value {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(data.len() * 2);
    for byte in data {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    Value::String(text)
}

fn read_memory(value: &Value) -> Result<MemoryInfo> {
    check_keys(
        value,
        &[
            "usage",
            "total_physical",
            "free_physical",
            "total_paging",
            "free_paging",
            "total_virtual",
            "free_virtual",
        ],
        "memory",
    )?;
    Ok(MemoryInfo {
        usage: unsigned32(member(value, "usage")?, "memory.usage")?,
        total_physical: unsigned(
            member(value, "total_physical")?,
            "memory.total_physical",
            u64::MAX,
        )?,
        free_physical: unsigned(
            member(value, "free_physical")?,
            "memory.free_physical",
            u64::MAX,
        )?,
        total_paging: unsigned(
            member(value, "total_paging")?,
            "memory.total_paging",
            u64::MAX,
        )?,
        free_paging: unsigned(
            member(value, "free_paging")?,
            "memory.free_paging",
            u64::MAX,
        )?,
        total_virtual: unsigned(
            member(value, "total_virtual")?,
            "memory.total_virtual",
            u64::MAX,
        )?,
        free_virtual: unsigned(
            member(value, "free_virtual")?,
            "memory.free_virtual",
            u64::MAX,
        )?,
    })
}

fn read_cpus(input: &Value) -> Result<Vec<CpuInfo>> {
    let mut cpus = Vec::new();
    for entry in array(input, "cpus", MAX_ENTRIES)? {
        check_keys(
            entry,
            &["name", "manufacturer", "number_cores", "number_logical"],
            "cpus",
        )?;
        // Field order matches the C++ assignments.
        let name = bytes(member(entry, "name")?, "cpus.name")?;
        let manufacturer = bytes(member(entry, "manufacturer")?, "cpus.manufacturer")?;
        let number_cores = unsigned32(member(entry, "number_cores")?, "cpus.number_cores")?;
        let number_logical = unsigned32(member(entry, "number_logical")?, "cpus.number_logical")?;
        cpus.push(CpuInfo {
            name,
            manufacturer,
            number_cores,
            number_logical,
        });
    }
    Ok(cpus)
}

fn read_gpus(input: &Value) -> Result<Vec<GpuInfo>> {
    let mut gpus = Vec::new();
    for entry in array(input, "gpus", MAX_ENTRIES)? {
        check_keys(
            entry,
            &["name", "memory_size", "driver_version", "manufacturer"],
            "gpus",
        )?;
        let name = bytes(member(entry, "name")?, "gpus.name")?;
        let memory_size = signed(member(entry, "memory_size")?, "gpus.memory_size")?;
        let driver_version = bytes(member(entry, "driver_version")?, "gpus.driver_version")?;
        let manufacturer = bytes(member(entry, "manufacturer")?, "gpus.manufacturer")?;
        gpus.push(GpuInfo {
            name,
            memory_size,
            driver_version,
            manufacturer,
        });
    }
    Ok(gpus)
}

/// The address, length and capacity of every buffer the output arguments own, taken
/// once after decoding. Only equality with a later snapshot is reported, so the
/// reply never depends on an address.
#[derive(PartialEq, Eq)]
struct Storage(Vec<(*const (), usize, usize)>);

impl Storage {
    fn of(cpus: &Vec<CpuInfo>, gpus: &Vec<GpuInfo>) -> Self {
        let mut buffers = vec![(cpus.as_ptr().cast::<()>(), cpus.len(), cpus.capacity())];
        let text = |s: &Vec<u8>| (s.as_ptr().cast::<()>(), s.len(), s.capacity());
        for cpu in cpus {
            buffers.push(text(&cpu.name));
            buffers.push(text(&cpu.manufacturer));
        }
        buffers.push((gpus.as_ptr().cast::<()>(), gpus.len(), gpus.capacity()));
        for gpu in gpus {
            buffers.push(text(&gpu.name));
            buffers.push(text(&gpu.driver_version));
            buffers.push(text(&gpu.manufacturer));
        }
        Self(buffers)
    }
}

fn state(memory: &MemoryInfo, cpus: &[CpuInfo], gpus: &[GpuInfo]) -> Value {
    let cpus: Vec<Value> = cpus
        .iter()
        .map(|cpu| {
            json!({
                "manufacturer": hex(&cpu.manufacturer),
                "name": hex(&cpu.name),
                "number_cores": cpu.number_cores,
                "number_logical": cpu.number_logical,
            })
        })
        .collect();
    let gpus: Vec<Value> = gpus
        .iter()
        .map(|gpu| {
            json!({
                "driver_version": hex(&gpu.driver_version),
                "manufacturer": hex(&gpu.manufacturer),
                "memory_size": gpu.memory_size,
                "name": hex(&gpu.name),
            })
        })
        .collect();
    json!({
        "cpus": cpus,
        "gpus": gpus,
        "memory": {
            "free_paging": memory.free_paging,
            "free_physical": memory.free_physical,
            "free_virtual": memory.free_virtual,
            "total_paging": memory.total_paging,
            "total_physical": memory.total_physical,
            "total_virtual": memory.total_virtual,
            "usage": memory.usage,
        },
    })
}

#[derive(Clone, Copy)]
enum Step {
    Memory,
    Cpu,
    Gpu,
    Reconstruct,
}

impl Step {
    fn name(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Reconstruct => "reconstruct",
        }
    }
}

fn run_sysinfo(input: &Value) -> Result<Value> {
    check_keys(input, &["op", "memory", "cpus", "gpus", "steps"], "")?;
    let mut memory = read_memory(member(input, "memory")?)?;
    let mut cpus = read_cpus(input)?;
    let mut gpus = read_gpus(input)?;
    let steps = array(input, "steps", MAX_STEPS)?
        .iter()
        .map(|step| match step.as_str() {
            Some("memory") => Ok(Step::Memory),
            Some("cpu") => Ok(Step::Cpu),
            Some("gpu") => Ok(Step::Gpu),
            Some("reconstruct") => Ok(Step::Reconstruct),
            _ => Err("unknown step".to_owned()),
        })
        .collect::<Result<Vec<Step>>>()?;

    let initial = Storage::of(&cpus, &gpus);
    let before = state(&memory, &cpus, &gpus);
    let mut observed = Vec::with_capacity(steps.len());
    let mut sysinfo = Sysinfo::new();
    for step in steps {
        let returned = match step {
            Step::Reconstruct => {
                // Destroy the object, then construct a new one, as the C++
                // std::optional reset() and emplace() do.
                drop(sysinfo);
                sysinfo = Sysinfo::new();
                None
            }
            Step::Memory => Some(sysinfo.get_memory_info(&mut memory)),
            Step::Cpu => Some(sysinfo.get_cpu_info(&mut cpus)),
            Step::Gpu => Some(sysinfo.get_gpu_info(&mut gpus)),
        };
        // Keys are inserted in nlohmann's sorted order, so the serialized reply is
        // byte-identical to the C++ adapter's (serde_json here preserves insertion
        // order).
        let mut entry = Map::new();
        if let Some(returned) = returned {
            entry.insert("returned".into(), returned.into());
        }
        entry.insert("state".into(), state(&memory, &cpus, &gpus));
        entry.insert("step".into(), step.name().into());
        entry.insert(
            "storage_unchanged".into(),
            (Storage::of(&cpus, &gpus) == initial).into(),
        );
        observed.push(Value::Object(entry));
    }
    drop(sysinfo);
    Ok(json!({
        "after_destroy": {
            "state": state(&memory, &cpus, &gpus),
            "storage_unchanged": Storage::of(&cpus, &gpus) == initial,
        },
        "before": before,
        "steps": observed,
    }))
}

/// Run one decoded `input` object.
pub fn run(input: &Value) -> Result<Value> {
    let op = input
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| "expected string: op".to_owned())?;
    match op {
        "valid_3dconnexion_driver_version" => {
            check_keys(input, &["op"], "")?;
            Ok(json!({"returned": drivers::valid_3dconnexion_driver_version()}))
        }
        "sysinfo" => run_sysinfo(input),
        _ => Err("unknown operation".into()),
    }
}
