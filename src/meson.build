sources = [
    'test/Boot0001-8be4df61-93ca-11d2-aa0d-00e098032b8c',
    'main.rs',
    'efiboot.rs',
    'efidevicepath.rs',
    'efiloadoption.rs',
    'efivars.rs',
]

cargo_script = find_program(join_paths(meson.source_root(), 'build-aux/cargo.sh'))
cargo_release = custom_target(
  'cargo-build',
  build_by_default: true,
  input: sources,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    cargo_script,
    meson.build_root(),
    meson.source_root(),
    '@OUTPUT@',
    get_option('buildtype'),
    meson.project_name(),
  ]
)
