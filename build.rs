fn main() {
    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .input_extern_file("src/layout.rs")
        .csharp_dll_name("cosmic_text")
        .csharp_dll_name_if("PRIMROSE_IOS", "__Internal")
        .csharp_namespace("Primrose.Graphics.Bespoke.Text.CosmicText.Native")
         //.csharp_imported_namespaces("MyLib")
        /* .csharp_type_rename(|rust_type_name| match rust_type_name {     // optional, default: `|x| x`
            //"FfiConfiguration" => "Configuration".into(),
            _ => x,
        })*/
        .generate_csharp_file("../PrimroseEngine/Engine/Source/Graphics/Bespoke/Text/CosmicText/CosmicText.g.cs")
       
        .unwrap();
} 