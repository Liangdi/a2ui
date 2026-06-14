use a2ui_gallery::app::GalleryApp;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = GalleryApp::new()?;
    app.run()?;
    Ok(())
}
