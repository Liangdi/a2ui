//! Basic catalog — registers all 14 functions and 18 components.

use crate::core::catalog::basic_functions::build_basic_functions;
use crate::core::catalog::Catalog;
use crate::tui::component_impl::{ComponentRegistry, build_registry};
use crate::tui::components::audio_player::AudioPlayerComponent;
use crate::tui::components::button::ButtonComponent;
use crate::tui::components::card::CardComponent;
use crate::tui::components::checkbox::CheckBoxComponent;
use crate::tui::components::choice_picker::ChoicePickerComponent;
use crate::tui::components::column::ColumnComponent;
use crate::tui::components::date_time_input::DateTimeInputComponent;
use crate::tui::components::divider::DividerComponent;
use crate::tui::components::icon::IconComponent;
use crate::tui::components::image::ImageComponent;
use crate::tui::components::list::ListComponent;
use crate::tui::components::modal::ModalComponent;
use crate::tui::components::row::RowComponent;
use crate::tui::components::slider::SliderComponent;
use crate::tui::components::tabs::TabsComponent;
use crate::tui::components::text::TextComponent;
use crate::tui::components::text_field::TextFieldComponent;
use crate::tui::components::video::VideoComponent;

/// Build the basic catalog with all 14 functions.
pub fn build_basic_catalog() -> Catalog {
    let mut catalog =
        Catalog::new("https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json");
    for func in build_basic_functions() {
        catalog = catalog.with_function(func);
    }
    catalog
}

/// Build the component registry with all 18 basic catalog components.
pub fn build_basic_registry() -> ComponentRegistry {
    build_registry(vec![
        Box::new(TextComponent),
        Box::new(RowComponent),
        Box::new(ColumnComponent),
        Box::new(ButtonComponent),
        Box::new(TextFieldComponent),
        Box::new(CardComponent),
        Box::new(DividerComponent),
        Box::new(ListComponent),
        Box::new(CheckBoxComponent),
        Box::new(IconComponent),
        Box::new(TabsComponent),
        Box::new(ModalComponent),
        Box::new(SliderComponent),
        Box::new(ChoicePickerComponent),
        Box::new(DateTimeInputComponent),
        Box::new(ImageComponent),
        Box::new(VideoComponent),
        Box::new(AudioPlayerComponent),
    ])
}
