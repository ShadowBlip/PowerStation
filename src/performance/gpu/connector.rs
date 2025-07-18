use std::fs;
use zbus::fdo;
use zbus_macros::interface;

/// Represents the data contained in /sys/class/drm/cardX-YYYY
pub struct Connector {
    pub name: String,
    pub path: String,
}

#[interface(name = "org.shadowblip.GPU.Card.Connector")]
impl Connector {
    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn path(&self) -> String {
        self.path.clone()
    }

    #[zbus(property)]
    fn id(&self) -> fdo::Result<u32> {
        let path = format!("{0}/{1}", self.path(), "connector_id");
        let result = fs::read_to_string(path);
        let result = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?;
        let id = result.trim();

        // Convert the ID to an integer
        let id = id
            .parse::<u32>()
            // convert the ParseIntError to a zbus::fdo::Error
            .map_err(|err| fdo::Error::Failed(err.to_string()))?;

        Ok(id)
    }

    #[zbus(property)]
    fn enabled(&self) -> fdo::Result<bool> {
        let path = format!("{0}/{1}", self.path(), "enabled");
        let result = fs::read_to_string(path);
        let status = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        Ok(status == "enabled")
    }

    #[zbus(property)]
    fn modes(&self) -> fdo::Result<Vec<String>> {
        let mut modes: Vec<String> = Vec::new();
        let path = format!("{0}/{1}", self.path(), "modes");
        let result = fs::read_to_string(path);
        let lines = result.map_err(|err| fdo::Error::IOError(err.to_string()))?;
        let lines = lines.split("\n");

        // Add each available mode to the list of modes
        for line in lines {
            let mode = line.trim().to_string();
            if mode.is_empty() {
                continue;
            }
            modes.push(mode);
        }

        Ok(modes)
    }

    #[zbus(property)]
    fn status(&self) -> fdo::Result<String> {
        let path = format!("{0}/{1}", self.path(), "status");
        let result = fs::read_to_string(path);
        let status = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        Ok(status)
    }

    #[zbus(property, name = "DPMS")]
    fn dpms(&self) -> fdo::Result<bool> {
        let path = format!("{0}/{1}", self.path(), "dpms");
        let result = fs::read_to_string(path);
        let status = result
            // convert the std::io::Error to a zbus::fdo::Error
            .map_err(|err| fdo::Error::IOError(err.to_string()))?
            .trim()
            .to_lowercase();

        Ok(status == "on")
    }
}
