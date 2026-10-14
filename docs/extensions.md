# Explorer extensions (Rust, protocol v1)

Explorer's core owns navigation, file operations, thumbnailing and portable
image annotations. Site-specific scrapers, batch organizers and importers
belong in **external Rust executables**, not dependencies of the core GUI.

An extension is an explicitly invoked local program. Explorer does not load a
shared library, embed a scripting runtime, evaluate source files or poll sites.

## Installing an extension

1. Build a Rust binary, for example:
   `cargo build --release --manifest-path scripts/extensions/import_annotations/Cargo.toml`
2. Create `~/.config/explorer/extensions`.
3. Copy the example manifest to that directory, rename it
   `import_annotations.json`, and set `executable` to the **absolute path**
   of the compiled binary.
4. Restart Explorer or invoke `:scripts`. Use
   `:script import_annotations collection.json` inside a folder containing
   images and a matching scraper-generated collection JSON file.

The `collection.example.json` in the example crate documents the input
format. A Pinterest or Cosmos scraper can create images plus this manifest,
or a future extension can download directly into the open folder. Official
Cosmos ZIP archives may include images without captions or source URLs; never
invent those missing fields.

## Protocol

Explorer sends one JSON object to the executable's **stdin**:

```json
{
  "api_version": 1,
  "action": "run",
  "directory": "/home/user/Pictures/References",
  "selected_paths": ["/home/user/Pictures/References/ref.jpg"],
  "arguments": "collection.json"
}
```

The extension must print one UTF-8 JSON object to **stdout**:

```json
{
  "message": "Imported references",
  "refresh": true,
  "updates": [
    {
      "path": "ref.jpg",
      "caption": "Poster reference",
      "source": "https://example.com/source",
      "tags": ["poster", "design"]
    }
  ]
}
```

Metadata update fields (all optional): `title`, `caption`, `source`,
`notes`, `tags` (array of strings), `creator`. Explorer merges only supplied
fields, preserving unrelated annotations. `path` must resolve to a real file
inside the open directory. Existing pixel data remains unchanged.

Exit nonzero to report a failure; diagnostic text belongs on stderr.
Output larger than 1 MiB is rejected. The core refreshes its folder after
a script finishes.

## Filesystem layout

```
References/
  photo.jpg
  .explorer/
    metadata/
      70686f746f2e6a7067.json
    board.json                 # optional future board layout
```

The hex filename is the byte representation of the original filename. This
keeps local filenames (including non-UTF-8 paths) distinct and avoids escaped
names or a required database. When moving or renaming files through Explorer,
the companion annotations are preserved. Copying the directory with a normal
file utility also copies the hidden `.explorer` directory.

**Embedding vs sidecars:** v1 currently writes JSON sidecars, not native XMP
inside JPEG/PNG/WebP. Sidecars avoid image recompression and preserve existing
embedded EXIF/XMP; standards-compliant embedded XMP synchronization is a
separate capability and is not claimed here.

## Extension security

Only install executables you trust. These are native programs with the same OS
permissions as Explorer; the extension manifest is **not** a sandbox or security
boundary. They are never launched automatically, and the manifest must name a
specific executable rather than a shell command. A Rust `Command` launches
that executable directly; arguments are passed as JSON, not shell-interpolated.

Site-specific network access and authentication belong to the extension.
Keep credentials outside exported annotations and do not store session cookies
inside board files.

## Core file-operation capabilities

Protocol v1 responses can also contain an `operations` array. Each action
uses Explorer's existing no-overwrite file operations and is restricted to the
open directory and its descendants.

```json
{
  "operations": [
    { "kind": "create_folder", "name": "Images" },
    { "kind": "move", "path": "photo.jpg", "folder": "Images" },
    { "kind": "copy", "path": "poster.png", "folder": "Images" }
  ]
}
```

Actions run in order. Failures report an error, but earlier successful actions
are not rolled back yet. The `organize_folder` example demonstrates this:
build it with `cargo build --release --manifest-path scripts/extensions/organize_folder/Cargo.toml`,
install its manifest with the absolute executable path, and use
`:script organize_folder all` to process the open folder (or omit `all`
to process the current selection). Image sidecars are preserved by the core.

## Optional embedded XMP synchronization

Metadata is local-first and stored in `.explorer/metadata/*.json`. Install
the distribution's **Exiv2** command-line utility to enable `:xmp_read`
(import embedded title, caption, tags, creator and source) or `:xmp_write`
(write those standard XMP fields into the selected image). Explorer invokes
the executable directly without a shell and writes to a sibling temporary copy
before replacing the original. Notes and board coordinates stay in the JSON
sidecar, because they do not have interoperable standard XMP fields.

XMP write is explicit, never automatic. Back up valuable originals before
batch use, because supported metadata fields and image formats vary across
Exiv2 versions; unsuitable or read-only files should retain their sidecars.
