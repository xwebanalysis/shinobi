// Production build: the Rust backend serves this bundle on the same origin, so
// relative URLs keep the app working behind Docker, a LAN IP or a reverse proxy
// without hardcoding host:port anywhere. Extractor health is probed through the
// backend proxy /api/python/docs (see ApiService.extractorStatus).
export const environment = {
  production: true,
  apiBaseUrl: '',
};
