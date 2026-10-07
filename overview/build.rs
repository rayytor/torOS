// Pack the icons (data/icons) into the program. They are symbolic icons drawn
// like the picker's and the screenshot tool's: shapes to fill, no lines
// (close is the screenshot tool's, the magnifier was outlined by hand).
fn main() {
    glib_build_tools::compile_resources(&["data"], "data/icons.gresource.xml", "icons.gresource");
}
