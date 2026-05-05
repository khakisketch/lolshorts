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
    use sysinfo::{Disks, System};

    let sys = System::new_all();

    let ram_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
    let ram_sufficient = ram_gb >= 4.0;

    let disks = Disks::new_with_refreshed_list();
    let disk_free_gb = disks.iter().map(|d| d.available_space()).max().unwrap_or(0) as f64
        / 1024.0
        / 1024.0
        / 1024.0;
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
