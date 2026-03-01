use dds::cmn::{
    teDiskType, tsCpuStatus, tsCpuTempInfo, tsDiskStatus, tsMemoryStatus, tsProcessInfo,
    tsSystemInfoMsg,
};
use itertools::Itertools;
use sysinfo::{
    Components, DiskKind, DiskRefreshKind, Disks, ProcessRefreshKind, ProcessesToUpdate, System,
    UpdateKind,
};

pub struct SystemStructs {
    system: System,
    components: Components,
    disks: Disks,
}

impl SystemStructs {
    pub fn new(system: System, components: Components, disks: Disks) -> Self {
        SystemStructs {
            system,
            components,
            disks,
        }
    }
}

pub fn gather_system_info(system_structs: &mut SystemStructs) -> tsSystemInfoMsg {
    system_structs.system.refresh_memory();
    let mem_status = tsMemoryStatus::new(
        system_structs.system.used_memory(),
        system_structs.system.total_memory(),
    );

    // Needs updates for intel
    let cpu_temps = system_structs
        .components
        .iter_mut()
        .filter(|component| component.label().contains("k10temp"))
        .filter_map(|component| {
            component.refresh();
            if let Some(id) = component.id()
                && let Some(temp) = component.temperature()
            {
                Some(tsCpuTempInfo::new(id.to_string(), temp))
            } else {
                None
            }
        })
        .collect::<Vec<tsCpuTempInfo>>();

    system_structs.system.refresh_cpu_all();
    let cpu_freqs = system_structs
        .system
        .cpus()
        .iter()
        .map(|cpu| cpu.frequency())
        .collect::<Vec<u64>>();

    let cpu_status = tsCpuStatus::new(
        system_structs.system.global_cpu_usage(),
        system_structs.system.cpus().len() as u16,
        system_structs.system.cpus()[0].brand().to_string(),
        mean(&cpu_freqs),
        cpu_temps,
        cpu_freqs,
    );

    let disk_refresh = DiskRefreshKind::nothing();
    let disk_refresh = disk_refresh.with_storage();
    let disk_status = system_structs
        .disks
        .iter_mut()
        .filter_map(|disk| match disk.kind() {
            DiskKind::HDD => {
                disk.refresh_specifics(disk_refresh);
                Some(tsDiskStatus::new(
                    disk.name()
                        .to_os_string()
                        .into_string()
                        .expect("Unable to convert Disk Name to String"),
                    disk.mount_point().to_string_lossy().into_owned(),
                    teDiskType::eeHDD,
                    disk.total_space() - disk.available_space(),
                    disk.total_space(),
                ))
            }
            DiskKind::SSD => {
                disk.refresh_specifics(disk_refresh);
                Some(tsDiskStatus::new(
                    disk.name()
                        .to_os_string()
                        .into_string()
                        .expect("Unable to convert Disk Name to String"),
                    disk.mount_point().to_string_lossy().into_owned(),
                    teDiskType::eeSSD,
                    disk.total_space() - disk.available_space(),
                    disk.total_space(),
                ))
            }
            DiskKind::Unknown(_) => None,
        })
        .collect::<Vec<tsDiskStatus>>();

    let proc_refresh_kind = ProcessRefreshKind::nothing();
    let proc_refresh_kind = proc_refresh_kind
        .with_disk_usage()
        .with_cpu()
        .with_memory()
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet);
    system_structs.system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        proc_refresh_kind,
    );
    let process_info = system_structs
        .system
        .processes()
        .iter()
        .map(|(pid, proc)| {
            let cmd = proc
                .cmd()
                .to_vec()
                .iter()
                .map(|option| option.clone().into_string().unwrap_or_default())
                .intersperse(" ".to_string())
                .collect::<String>();
            let exe = match proc.exe() {
                Some(path) => path.to_string_lossy().to_string(),
                None => "Unknown".to_string(),
            };
            tsProcessInfo {
                mnPid: pid.as_u32(),
                mcCommandLine: cmd,
                mrCurCpuUsage: proc.cpu_usage(),
                mnBytesWritten: proc.disk_usage().total_written_bytes,
                mnBytesRead: proc.disk_usage().total_read_bytes,
                mcExe: exe,
                mnCurMemUsage: proc.memory(),
                mnCurVirtualMemoryUsage: proc.virtual_memory(),
            }
        })
        .collect::<Vec<tsProcessInfo>>();

    tsSystemInfoMsg::new(cpu_status, mem_status, disk_status, process_info)
}

fn mean(numbers: &[u64]) -> f32 {
    let sum: u64 = numbers.iter().sum();
    sum as f32 / numbers.len() as f32
}
