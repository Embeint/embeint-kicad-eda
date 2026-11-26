// SPDX-License-Identifier: GPL-3.0-or-later
//! `KIPLATFORM::SYSINFO` for Unix (libs/kiplatform/os/unix/sysinfo.cpp).
//!
//! Minimum co-port of the data types from `kiplatform/sysinfo.h` and the class
//! declaration from `kiplatform/sysinfo_gtk.h`; both headers stay owned by
//! [`crate::HEADER_UNIT`].
//!
//! The C++ structs are aggregates whose scalar members are left uninitialized by
//! default construction (`std::string` members start empty). Rust has no
//! uninitialized value, so these types deliberately have no `Default`: every caller
//! supplies every field, as the oracle fixtures do before any observation.
//! `std::string` members are byte vectors because a C++ string may hold NUL and
//! bytes that are not UTF-8.
//!
//! `SYSINFO_BASE` (pure virtual, inherited privately by `SYSINFO`, so it is not a
//! usable base outside the class) is not ported; the overrides are inherent methods.

/// `KIPLATFORM::GPU_INFO`.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
