// Runtime API base URL. The hostname is resolved at runtime so the app works
// from localhost, a LAN address or a tunnel; only the backend port is fixed.
// Development: `npm start` serves the UI on :4260 and talks to the Rust API on
// :8060 (CORS allows localhost/LAN origins). The dev-server proxy also forwards
// relative /api calls, but the client uses this absolute base by design.
const host =
  typeof window !== 'undefined' && window.location?.hostname
    ? window.location.hostname
    : 'localhost';

export const environment = {
  production: false,
  apiBaseUrl: `http://${host}:8060`,
};
