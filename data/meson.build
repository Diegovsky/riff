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

appstreamcli = find_program('appstreamcli', required: false)
if appstreamcli.found()
  test(
    'Validate AppStream metainfo',
    appstreamcli,
    # --no-net keeps CLI from phoning home; --pedantic catches style issues
    args: ['validate', '--no-net', '--pedantic',
          meson.current_source_dir() / 'dev.diegovsky.Riff.metainfo.xml']
  )
endif
