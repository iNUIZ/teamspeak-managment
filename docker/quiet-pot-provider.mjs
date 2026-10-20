// bgutil's diagnostic output can contain anonymous tokens and proxy credentials.
// Never forward that output into Manager/Docker logs. Readiness is exposed by
// the local /ping probe, not by copying upstream stdout into the panel.
for (const method of ['log', 'info', 'warn', 'error', 'debug', 'trace']) {
  console[method] = () => {};
}
try {
  await import('./server/build/main.js');
} catch {
  process.stderr.write('PO Token provider could not start. Check the packaged runtime.\n');
  process.exitCode = 1;
}
