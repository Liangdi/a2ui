//! Basic catalog — registers all 14 functions and 18 components.

use crate::component_impl::{ComponentRegistry, build_registry};
use crate::components::audio_player::AudioPlayerComponent;
use crate::components::button::ButtonComponent;
use crate::components::card::CardComponent;
use crate::components::checkbox::CheckBoxComponent;
use crate::components::choice_picker::ChoicePickerComponent;
use crate::components::column::ColumnComponent;
use crate::components::date_time_input::DateTimeInputComponent;
use crate::components::divider::DividerComponent;
use crate::components::icon::IconComponent;
use crate::components::image::ImageComponent;
use crate::components::list::ListComponent;
use crate::components::modal::ModalComponent;
use crate::components::row::RowComponent;
use crate::components::slider::SliderComponent;
use crate::components::tabs::TabsComponent;
use crate::components::text::TextComponent;
use crate::components::text_field::TextFieldComponent;
use crate::components::video::VideoComponent;
use a2ui_base::catalog::Catalog;
use a2ui_base::catalog::basic_functions::build_basic_functions;

/// Build the basic catalog with all 14 functions and 18 components.
pub fn build_basic_catalog() -> Catalog {
    let mut catalog =
        Catalog::new("https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json");
    for func in build_basic_functions() {
        catalog = catalog.with_function(func);
    }
    catalog
        .with_component(Box::new(TextComponent))
        .with_component(Box::new(RowComponent))
        .with_component(Box::new(ColumnComponent))
        .with_component(Box::new(ButtonComponent))
        .with_component(Box::new(TextFieldComponent))
        .with_component(Box::new(CardComponent))
        .with_component(Box::new(DividerComponent))
        .with_component(Box::new(ListComponent))
        .with_component(Box::new(CheckBoxComponent))
        .with_component(Box::new(IconComponent))
        .with_component(Box::new(TabsComponent))
        .with_component(Box::new(ModalComponent))
        .with_component(Box::new(SliderComponent))
        .with_component(Box::new(ChoicePickerComponent))
        .with_component(Box::new(DateTimeInputComponent))
        .with_component(Box::new(ImageComponent))
        .with_component(Box::new(VideoComponent))
        .with_component(Box::new(AudioPlayerComponent))
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
