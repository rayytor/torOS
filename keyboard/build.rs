// Pack the icons (data/icons, drawn by icons.py) into the program.
fn main() {
    glib_build_tools::compile_resources(&["data"], "data/icons.gresource.xml", "icons.gresource");
}
