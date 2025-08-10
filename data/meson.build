install_data('dev.diegovsky.Riff.desktop',
  install_dir: get_option('datadir') / 'applications'
)

install_subdir('hicolor',
  install_dir: get_option('datadir') / 'icons'
)

install_data('dev.diegovsky.Riff.metainfo.xml',
  install_dir: get_option('datadir') / 'metainfo'
)

install_data('dev.diegovsky.Riff.gschema.xml',
  install_dir: get_option('datadir') / 'glib-2.0/schemas'
)

compile_schemas = find_program('glib-compile-schemas', required: true)
if compile_schemas.found()
  test('Validate schema file', compile_schemas,
    args: ['--strict', '--dry-run', meson.current_source_dir()]
  )
endif

appstream_util = find_program('appstream-util', required: false)
if appstream_util.found()
  test(
    'Validate appstream appdata',
    appstream_util,
    args: ['validate-relax', meson.current_source_dir() / 'dev.diegovsky.Riff.metainfo.xml']
  )
endif
