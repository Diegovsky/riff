project(
  'riff',
  version: '26.07.1',
  meson_version: '>= 0.59.0',
  default_options: ['warning_level=2', 'buildtype=release'],
)

fs = import('fs')

generated_spotify_api = meson.project_source_root() / 'generated' / 'spotify-api'
if not fs.is_dir(generated_spotify_api) or not fs.is_file(generated_spotify_api / 'Cargo.toml')
  error(
    'Generated Spotify API crate not found at @0@\n\n'.format(generated_spotify_api) +
    'Run the following to generate it (requires Java):\n' +
    '  ./scripts/generate-spotify-api.sh\n' +
    'or:\n' +
    '  just generate-spotify-api\n'
  )
endif

subdir('data')
subdir('po')
subdir('src')

flatpak_cargo_generator = find_program(meson.project_source_root() / 'scripts/flatpak-cargo-generator.py')

cargo_sources = custom_target(
  'cargo-update-sources',
  build_by_default: false,
  output: 'cargo-sources.json',
  input: meson.project_source_root() / 'Cargo.lock',
  command: [
    flatpak_cargo_generator,
    '@INPUT@',
    '-o', '@SOURCE_ROOT@/flatpak/cargo-sources.json'
  ]
)
