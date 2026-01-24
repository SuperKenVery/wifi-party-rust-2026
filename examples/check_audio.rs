use cpal::traits::{DeviceTrait, HostTrait};

fn main() {
    let host = cpal::default_host();

    if let Some(device) = host.default_output_device() {
        println!(
            "Default output device: {}",
            device.name().unwrap_or_default()
        );

        if let Ok(config) = device.default_output_config() {
            println!("Default config: {:#?}", config);
        }

        if let Ok(configs) = device.supported_output_configs() {
            println!("\nSupported configs:");
            for config in configs {
                println!("  {:?}", config);
            }
        }
    }
}
