addEventListener('fetch', event => {
  event.respondWith(handleRequest(event.request))
})

const CORS = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type, auth-key, client-version, device-id, language, User-Agent',
}

async function handleRequest(request) {
  const url = new URL(request.url)
  const path = url.pathname

  if (request.method === 'OPTIONS') {
    return new Response(null, { headers: CORS })
  }

  // ============ VinBus / BusMap proxy ============
  if (path.startsWith('/api/vinbus/')) {
    const target = 'https://api.busmap.city' + path.replace('/api/vinbus', '')
    try {
      const resp = await fetch(target, {
        headers: {
          'package-name': 'com.t7.busmaphn',
          'client-version': 'android|20620',
          'device-id': 'worker-' + Date.now(),
          'language': 'vi',
          'Accept': 'application/json',
        },
      })
      const body = await resp.text()
      return new Response(body, {
        status: resp.status,
        headers: { ...CORS, 'Content-Type': 'application/json' },
      })
    } catch (e) {
      return new Response(JSON.stringify({ error: e.message }), {
        status: 502,
        headers: { ...CORS, 'Content-Type': 'application/json' },
      })
    }
  }

  // ============ BusMap VN proxy (estimate_bus_to_station) ============
  if (path.startsWith('/api/busmap-vn/')) {
    const target = 'https://api.busmap.vn' + path.replace('/api/busmap-vn', '')
    try {
      const resp = await fetch(target, {
        headers: {
          'auth-key': 'ViNbUs2@2lElectricDtPZG+ulbseO1WGN',
          'client-version': 'android|690',
          'device-id': 'worker-' + Date.now(),
          'language': 'en',
          'Accept': 'application/json',
          'User-Agent': 'VinBus/1.0 (Android)',
        },
      })
      const body = await resp.text()
      return new Response(body, {
        status: resp.status,
        headers: { ...CORS, 'Content-Type': 'application/json' },
      })
    } catch (e) {
      return new Response(JSON.stringify({ error: e.message }), {
        status: 502,
        headers: { ...CORS, 'Content-Type': 'application/json' },
      })
    }
  }

  // ============ iHanoi traffic cameras proxy ============
  if (path.startsWith('/api/ihanoi/')) {
    const target = 'https://cds.hanoi.gov.vn' + path.replace('/api/ihanoi', '')
    try {
      const resp = await fetch(target, {
        headers: { 'Accept': 'application/json', 'User-Agent': 'Mozilla/5.0' },
      })
      const body = await resp.text()
      return new Response(body, {
        status: resp.status,
        headers: { ...CORS, 'Content-Type': 'application/json' },
      })
    } catch (e) {
      return new Response(JSON.stringify({ error: e.message }), {
        status: 502,
        headers: { ...CORS, 'Content-Type': 'application/json' },
      })
    }
  }

  // ============ Serve frontend from GitHub Pages ============
  return fetch('https://projecthorizon993.github.io/bus-tracker-rust/', {
    headers: { 'Accept': 'text/html' },
  })
}
