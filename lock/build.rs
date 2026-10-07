// Pack the icons of the keys into the program. They are the on-screen
// keyboard's own (keyboard/data/icons, drawn by keyboard/icons.py).
fn main() {
    glib_build_tools::compile_resources(&["../keyboard/data"], "data/icons.gresource.xml", "icons.gresource");
}
