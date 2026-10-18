export async function resolveBotVoicePort(requested: number | null | undefined, serverInfo: () => Promise<unknown>): Promise<number> {
  if (requested != null && requested !== 0) {
    if (!Number.isInteger(requested) || requested < 1 || requested > 65535) throw new Error('Invalid voice port');
    return requested;
  }
  const info = await serverInfo();
  const record = (Array.isArray(info) ? info[0] : info) as { virtualserver_port?: unknown } | null;
  const port = Number(record?.virtualserver_port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Voice port could not be discovered. Set Voice Port manually.');
  return port;
}
