use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SystemRequirements {
    pub ram_gb: f64,
    pub ram_sufficient: bool,
    pub disk_free_gb: f64,
    pub disk_sufficient: bool,
    pub os_version: String,
    pub meets_minimum: bool,
}

pub fn check_system_requirements() -> SystemRequirements {
    use sysinfo::System;

    let sys = System::new_all();

    let ram_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let ram_sufficient = ram_gb >= 4.0;

    let disk_path = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
    let disk_free_gb = crate::utils::disk::query_disk_space(&disk_path)
        .map(|snapshot| snapshot.available_bytes as f64 / 1024.0 / 1024.0 / 1024.0)
        .unwrap_or(0.0);
    let disk_sufficient = disk_free_gb >= 2.0;

    let os_version = System::long_os_version().unwrap_or_default();

    SystemRequirements {
        ram_gb,
        ram_sufficient,
        disk_free_gb,
        disk_sufficient,
        os_version,
        meets_minimum: ram_sufficient && disk_sufficient,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_system_requirements() {
        let req = check_system_requirements();
        // RAM should be positive on any real machine
        assert!(req.ram_gb > 0.0);
        assert!(!req.os_version.is_empty());
    }
}
