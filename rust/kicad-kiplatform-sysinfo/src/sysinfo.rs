// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::SYSINFO` for Unix (libs/kiplatform/os/unix/sysinfo.cpp).
//!
//! The data types and [`SysinfoBase`] are the port of `kiplatform/sysinfo.h`, and
//! [`Sysinfo`]'s declaration that of `kiplatform/sysinfo_gtk.h`, both owned by
//! [`crate::HEADER_UNIT`] (first co-ported here by the Unix unit and adopted by the header
//! unit). `sysinfo.h` selects `sysinfo_gtk.h` on Unix; its Windows and macOS branches
//! (`sysinfo_windows.h`, `sysinfo_osx.h`) belong to the blocked unit
//! `libs.kiplatform.include.kiplatform.application.ui.p001` and are not modelled.
//!
//! The C++ structs are aggregates. Default-initialization (`GPU_INFO info;`) leaves their
//! scalar members indeterminate, which Rust cannot represent, so it is not modelled: every
//! caller supplies every field, as the oracle fixtures do before any observation.
//! [`Default`] is value-initialization (`GPU_INFO{}`): empty strings and zero scalars.
//! `std::string` members are byte vectors because a C++ string may hold NUL and bytes that
//! are not UTF-8.

/// `KIPLATFORM::GPU_INFO`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GpuInfo {
    /// `std::string Name`.
    pub name: Vec<u8>,
    /// `long long MemorySize` (64-bit on the reference LP64 target).
    pub memory_size: i64,
    /// `std::string DriverVersion`.
    pub driver_version: Vec<u8>,
    /// `std::string Manufacturer`.
    pub manufacturer: Vec<u8>,
}

/// `KIPLATFORM::CPU_INFO`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CpuInfo {
    /// `std::string Name`.
    pub name: Vec<u8>,
    /// `std::string Manufacturer`.
    pub manufacturer: Vec<u8>,
    /// `uint32_t NumberCores`.
    pub number_cores: u32,
    /// `uint32_t NumberLogical`.
    pub number_logical: u32,
}

/// `KIPLATFORM::MEMORY_INFO`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryInfo {
    /// `uint32_t Usage`.
    pub usage: u32,
    /// `uint64_t TotalPhysical`.
    pub total_physical: u64,
    /// `uint64_t FreePhysical`.
    pub free_physical: u64,
    /// `uint64_t TotalPaging`.
    pub total_paging: u64,
    /// `uint64_t FreePaging`.
    pub free_paging: u64,
    /// `uint64_t TotalVirtual`.
    pub total_virtual: u64,
    /// `uint64_t FreeVirtual`.
    pub free_virtual: u64,
}

/// `KIPLATFORM::SYSINFO_BASE`: the abstract interface every platform's `SYSINFO` implements.
///
/// On Unix `class SYSINFO : SYSINFO_BASE` inherits it privately, so outside the class a
/// `SYSINFO` cannot be used as a `SYSINFO_BASE`; [`Sysinfo`] therefore keeps its overrides
/// as inherent methods and does not implement this trait. A platform backend implements it.
pub trait SysinfoBase {
    /// `virtual bool GetGPUInfo( std::vector<GPU_INFO>& aGpuInfos ) = 0`.
    fn get_gpu_info(&mut self, gpu_infos: &mut Vec<GpuInfo>) -> bool;

    /// `virtual bool GetCPUInfo( std::vector<CPU_INFO>& aCpuInfos ) = 0`.
    fn get_cpu_info(&mut self, cpu_infos: &mut Vec<CpuInfo>) -> bool;

    /// `virtual bool GetMemoryInfo( MEMORY_INFO& aMemoryInfo ) = 0`.
    fn get_memory_info(&mut self, memory_info: &mut MemoryInfo) -> bool;
}

/// `KIPLATFORM::SYSINFO` (Unix). The class has no data members.
#[derive(Debug)]
pub struct Sysinfo {
    _members: (),
}

impl Sysinfo {
    /// `SYSINFO::SYSINFO()`: the pinned Unix body is empty.
    #[must_use]
    pub fn new() -> Self {
        Self { _members: () }
    }

    /// `SYSINFO::GetMemoryInfo( MEMORY_INFO& aMemoryInfo )`.
    ///
    /// The pinned Unix body is `return false;`: no information is retrieved and
    /// `memory_info` is neither read nor written.
    pub fn get_memory_info(&mut self, memory_info: &mut MemoryInfo) -> bool {
        let _ = memory_info;
        false
    }

    /// `SYSINFO::GetCPUInfo( std::vector<CPU_INFO>& aCpuInfos )`.
    ///
    /// The pinned Unix body is `return false;`: `cpu_infos` keeps its entries, order,
    /// contents and storage.
    pub fn get_cpu_info(&mut self, cpu_infos: &mut Vec<CpuInfo>) -> bool {
        let _ = cpu_infos;
        false
    }

    /// `SYSINFO::GetGPUInfo( std::vector<GPU_INFO>& aGpuInfos )`.
    ///
    /// The pinned Unix body is `return false;`: `gpu_infos` keeps its entries, order,
    /// contents and storage.
    pub fn get_gpu_info(&mut self, gpu_infos: &mut Vec<GpuInfo>) -> bool {
        let _ = gpu_infos;
        false
    }
}

impl Default for Sysinfo {
    /// `SYSINFO()` is the class's default constructor.
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Sysinfo {
    /// `SYSINFO::~SYSINFO()`: the pinned Unix body is empty and the class owns
    /// nothing, so destruction has no observable effect on anything.
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_initialization_is_empty_and_zero() {
        let gpu = GpuInfo::default();
        assert_eq!((gpu.name.len(), gpu.memory_size), (0, 0));
        let cpu = CpuInfo::default();
        assert_eq!((cpu.number_cores, cpu.number_logical), (0, 0));
        assert_eq!(MemoryInfo::default().free_virtual, 0);
    }

    #[test]
    fn queries_report_failure_and_leave_outputs_untouched() {
        let mut memory = MemoryInfo {
            usage: 7,
            total_physical: u64::MAX,
            free_physical: 1,
            total_paging: 2,
            free_paging: 3,
            total_virtual: 4,
            free_virtual: 5,
        };
        let mut cpus = vec![CpuInfo {
            name: b"a\0\xff".to_vec(),
            manufacturer: Vec::new(),
            number_cores: u32::MAX,
            number_logical: 0,
        }];
        let mut gpus = vec![GpuInfo {
            name: Vec::new(),
            memory_size: i64::MIN,
            driver_version: b"1".to_vec(),
            manufacturer: b"\x80".to_vec(),
        }];
        let (memory0, cpus0, gpus0) = (memory.clone(), cpus.clone(), gpus.clone());
        let (cpu_ptr, gpu_ptr) = (cpus.as_ptr(), gpus.as_ptr());
        let mut sysinfo = Sysinfo::new();
        assert!(!sysinfo.get_memory_info(&mut memory));
        assert!(!sysinfo.get_cpu_info(&mut cpus));
        assert!(!sysinfo.get_gpu_info(&mut gpus));
        drop(sysinfo);
        assert_eq!((memory, &cpus, &gpus), (memory0, &cpus0, &gpus0));
        assert_eq!((cpus.as_ptr(), gpus.as_ptr()), (cpu_ptr, gpu_ptr));
    }
}
