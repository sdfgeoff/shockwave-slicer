use iced::widget::{column, row, text, text_input};
use iced::{Element, Fill};
use shockwave_config::{Dimensions3, SlicerSettings};

#[derive(Clone, Debug)]
pub struct SettingsForm {
    layer_height_mm: String,
    voxel_x_mm: String,
    voxel_y_mm: String,
    voxel_z_mm: String,
    print_volume_x_mm: String,
    print_volume_y_mm: String,
    print_volume_z_mm: String,
    wall_count: String,
    infill_percentage: String,
    extrusion_width_mm: String,
    filament_diameter_mm: String,
    nozzle_temperature_c: String,
    bed_temperature_c: String,
    fan_speed_percent: String,
    global_z_offset_mm: String,
    printhead_clearance_height_mm: String,
    printhead_clearance_angle_degrees: String,
}

impl SettingsForm {
    pub fn from_settings(settings: &SlicerSettings) -> Self {
        Self {
            layer_height_mm: format_float(settings.slicing.layer_height_mm),
            voxel_x_mm: format_float(settings.field.voxel_size_mm.x),
            voxel_y_mm: format_float(settings.field.voxel_size_mm.y),
            voxel_z_mm: format_float(settings.field.voxel_size_mm.z),
            print_volume_x_mm: format_float(settings.printer.print_volume_mm.x),
            print_volume_y_mm: format_float(settings.printer.print_volume_mm.y),
            print_volume_z_mm: format_float(settings.printer.print_volume_mm.z),
            wall_count: settings.slicing.wall_count.to_string(),
            infill_percentage: format_float(settings.slicing.infill_percentage),
            extrusion_width_mm: format_float(settings.slicing.extrusion_width_mm),
            filament_diameter_mm: format_float(settings.material.filament_diameter_mm),
            nozzle_temperature_c: settings.material.nozzle_temperature_c.to_string(),
            bed_temperature_c: settings.material.bed_temperature_c.to_string(),
            fan_speed_percent: settings.material.fan_speed_percent.to_string(),
            global_z_offset_mm: format_float(settings.slicing.global_z_offset_mm),
            printhead_clearance_height_mm: format_float(
                settings.printer.obstruction.printhead_clearance_height_mm,
            ),
            printhead_clearance_angle_degrees: format_float(
                settings
                    .printer
                    .obstruction
                    .printhead_clearance_angle_degrees,
            ),
        }
    }

    pub fn update(&mut self, message: SettingsMessage) {
        match message {
            SettingsMessage::LayerHeight(value) => self.layer_height_mm = value,
            SettingsMessage::VoxelX(value) => self.voxel_x_mm = value,
            SettingsMessage::VoxelY(value) => self.voxel_y_mm = value,
            SettingsMessage::VoxelZ(value) => self.voxel_z_mm = value,
            SettingsMessage::PrintVolumeX(value) => self.print_volume_x_mm = value,
            SettingsMessage::PrintVolumeY(value) => self.print_volume_y_mm = value,
            SettingsMessage::PrintVolumeZ(value) => self.print_volume_z_mm = value,
            SettingsMessage::WallCount(value) => self.wall_count = value,
            SettingsMessage::InfillPercentage(value) => self.infill_percentage = value,
            SettingsMessage::ExtrusionWidth(value) => self.extrusion_width_mm = value,
            SettingsMessage::FilamentDiameter(value) => self.filament_diameter_mm = value,
            SettingsMessage::NozzleTemperature(value) => self.nozzle_temperature_c = value,
            SettingsMessage::BedTemperature(value) => self.bed_temperature_c = value,
            SettingsMessage::FanSpeed(value) => self.fan_speed_percent = value,
            SettingsMessage::GlobalZOffset(value) => self.global_z_offset_mm = value,
            SettingsMessage::PrintheadClearanceHeight(value) => {
                self.printhead_clearance_height_mm = value;
            }
            SettingsMessage::PrintheadClearanceAngle(value) => {
                self.printhead_clearance_angle_degrees = value;
            }
        }
    }

    pub fn apply_to_settings(&self, settings: &mut SlicerSettings) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut parsed = settings.clone();

        parsed.slicing.layer_height_mm = parse_f64(
            &self.layer_height_mm,
            "slicing.layer_height_mm",
            &mut errors,
        );
        parsed.field.voxel_size_mm = Dimensions3 {
            x: parse_f64(&self.voxel_x_mm, "field.voxel_size_mm.x", &mut errors),
            y: parse_f64(&self.voxel_y_mm, "field.voxel_size_mm.y", &mut errors),
            z: parse_f64(&self.voxel_z_mm, "field.voxel_size_mm.z", &mut errors),
        };
        parsed.printer.print_volume_mm = Dimensions3 {
            x: parse_f64(
                &self.print_volume_x_mm,
                "printer.print_volume_mm.x",
                &mut errors,
            ),
            y: parse_f64(
                &self.print_volume_y_mm,
                "printer.print_volume_mm.y",
                &mut errors,
            ),
            z: parse_f64(
                &self.print_volume_z_mm,
                "printer.print_volume_mm.z",
                &mut errors,
            ),
        };
        parsed.slicing.wall_count =
            parse_usize(&self.wall_count, "slicing.wall_count", &mut errors);
        parsed.slicing.infill_percentage = parse_f64(
            &self.infill_percentage,
            "slicing.infill_percentage",
            &mut errors,
        );
        parsed.slicing.extrusion_width_mm = parse_f64(
            &self.extrusion_width_mm,
            "slicing.extrusion_width_mm",
            &mut errors,
        );
        parsed.material.filament_diameter_mm = parse_f64(
            &self.filament_diameter_mm,
            "material.filament_diameter_mm",
            &mut errors,
        );
        parsed.material.nozzle_temperature_c = parse_u16(
            &self.nozzle_temperature_c,
            "material.nozzle_temperature_c",
            &mut errors,
        );
        parsed.material.bed_temperature_c = parse_u16(
            &self.bed_temperature_c,
            "material.bed_temperature_c",
            &mut errors,
        );
        parsed.material.fan_speed_percent = parse_u8(
            &self.fan_speed_percent,
            "material.fan_speed_percent",
            &mut errors,
        );
        parsed.slicing.global_z_offset_mm = parse_f64(
            &self.global_z_offset_mm,
            "slicing.global_z_offset_mm",
            &mut errors,
        );
        parsed.printer.obstruction.printhead_clearance_height_mm = parse_f64(
            &self.printhead_clearance_height_mm,
            "printer.obstruction.printhead_clearance_height_mm",
            &mut errors,
        );
        parsed.printer.obstruction.printhead_clearance_angle_degrees = parse_f64(
            &self.printhead_clearance_angle_degrees,
            "printer.obstruction.printhead_clearance_angle_degrees",
            &mut errors,
        );

        if errors.is_empty() {
            if let Err(validation_errors) = parsed.validate() {
                return Err(validation_errors);
            }
            *settings = parsed;
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn view(&self) -> Element<'_, SettingsMessage> {
        column![
            field(
                "Layer height (mm)",
                &self.layer_height_mm,
                SettingsMessage::LayerHeight
            ),
            vector_field(
                "Voxel size (mm)",
                &self.voxel_x_mm,
                SettingsMessage::VoxelX,
                &self.voxel_y_mm,
                SettingsMessage::VoxelY,
                &self.voxel_z_mm,
                SettingsMessage::VoxelZ,
            ),
            vector_field(
                "Print volume (mm)",
                &self.print_volume_x_mm,
                SettingsMessage::PrintVolumeX,
                &self.print_volume_y_mm,
                SettingsMessage::PrintVolumeY,
                &self.print_volume_z_mm,
                SettingsMessage::PrintVolumeZ,
            ),
            row![
                field("Wall count", &self.wall_count, SettingsMessage::WallCount),
                field(
                    "Infill (%)",
                    &self.infill_percentage,
                    SettingsMessage::InfillPercentage
                ),
                field(
                    "Extrusion width (mm)",
                    &self.extrusion_width_mm,
                    SettingsMessage::ExtrusionWidth
                ),
            ]
            .spacing(8),
            row![
                field(
                    "Filament diameter (mm)",
                    &self.filament_diameter_mm,
                    SettingsMessage::FilamentDiameter
                ),
                field(
                    "Nozzle temp (C)",
                    &self.nozzle_temperature_c,
                    SettingsMessage::NozzleTemperature
                ),
                field(
                    "Bed temp (C)",
                    &self.bed_temperature_c,
                    SettingsMessage::BedTemperature
                ),
            ]
            .spacing(8),
            row![
                field(
                    "Fan speed (%)",
                    &self.fan_speed_percent,
                    SettingsMessage::FanSpeed
                ),
                field(
                    "Global Z offset (mm)",
                    &self.global_z_offset_mm,
                    SettingsMessage::GlobalZOffset
                ),
            ]
            .spacing(8),
            row![
                field(
                    "Printhead clearance height (mm)",
                    &self.printhead_clearance_height_mm,
                    SettingsMessage::PrintheadClearanceHeight
                ),
                field(
                    "Printhead clearance angle (deg)",
                    &self.printhead_clearance_angle_degrees,
                    SettingsMessage::PrintheadClearanceAngle
                ),
            ]
            .spacing(8),
        ]
        .spacing(8)
        .into()
    }
}

#[derive(Clone, Debug)]
pub enum SettingsMessage {
    LayerHeight(String),
    VoxelX(String),
    VoxelY(String),
    VoxelZ(String),
    PrintVolumeX(String),
    PrintVolumeY(String),
    PrintVolumeZ(String),
    WallCount(String),
    InfillPercentage(String),
    ExtrusionWidth(String),
    FilamentDiameter(String),
    NozzleTemperature(String),
    BedTemperature(String),
    FanSpeed(String),
    GlobalZOffset(String),
    PrintheadClearanceHeight(String),
    PrintheadClearanceAngle(String),
}

fn field<'a>(
    label: &'a str,
    value: &'a str,
    on_change: impl Fn(String) -> SettingsMessage + 'a,
) -> Element<'a, SettingsMessage> {
    column![
        text(label),
        text_input(label, value)
            .on_input(on_change)
            .padding(6)
            .width(Fill),
    ]
    .spacing(2)
    .width(Fill)
    .into()
}

fn vector_field<'a>(
    label: &'a str,
    x: &'a str,
    on_x: impl Fn(String) -> SettingsMessage + 'a,
    y: &'a str,
    on_y: impl Fn(String) -> SettingsMessage + 'a,
    z: &'a str,
    on_z: impl Fn(String) -> SettingsMessage + 'a,
) -> Element<'a, SettingsMessage> {
    column![
        text(label),
        row![
            text_input("x", x).on_input(on_x).padding(6),
            text_input("y", y).on_input(on_y).padding(6),
            text_input("z", z).on_input(on_z).padding(6),
        ]
        .spacing(8),
    ]
    .spacing(2)
    .into()
}

fn parse_f64(value: &str, name: &str, errors: &mut Vec<String>) -> f64 {
    parse_value(value, name, errors)
}

fn parse_usize(value: &str, name: &str, errors: &mut Vec<String>) -> usize {
    parse_value(value, name, errors)
}

fn parse_u16(value: &str, name: &str, errors: &mut Vec<String>) -> u16 {
    parse_value(value, name, errors)
}

fn parse_u8(value: &str, name: &str, errors: &mut Vec<String>) -> u8 {
    parse_value(value, name, errors)
}

fn parse_value<T>(value: &str, name: &str, errors: &mut Vec<String>) -> T
where
    T: Default + std::str::FromStr,
{
    match value.trim().parse() {
        Ok(value) => value,
        Err(_) => {
            errors.push(format!("{name} must be a valid number"));
            T::default()
        }
    }
}

fn format_float(value: f64) -> String {
    format!("{value:.6}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}
