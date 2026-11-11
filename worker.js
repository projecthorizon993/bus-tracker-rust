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
  const searchParams = url.searchParams

  if (request.method === 'OPTIONS') {
    return new Response(null, { headers: CORS })
  }

  // Helper: forward POST form data to timbus.vn
  async function postToTimbus(endpoint, bodyParams) {
    const target = 'http://timbus.vn' + endpoint
    try {
      const resp = await fetch(target, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/x-www-form-urlencoded; charset=UTF-8',
          'User-Agent': 'Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/77.0.3833.0 Safari/537.36',
          'Accept': 'application/json, text/javascript, */*; q=0.01',
          'Accept-Language': 'en,vi-VN;q=0.9,vi;q=0.8,fr-FR;q=0.7,fr;q=0.6,en-US;q=0.5',
          'Origin': 'http://timbus.vn',
          'Referer': 'http://timbus.vn/',
          'X-Requested-With': 'XMLHttpRequest',
          'Host': 'timbus.vn',
        },
        body: bodyParams,
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

  // ============ Timbus API proxy (Hanoi bus backend) ============
  if (path.startsWith('/api/timbus/')) {
    const action = path.replace('/api/timbus/', '')

    if (action === 'search-bus') {
      const key = searchParams.get('q') || ''
      return postToTimbus('/Engine/Business/Search/action.ashx', 'act=searchfull&typ=1&key=' + encodeURIComponent(key))
    }
    if (action === 'search-stop') {
      const key = searchParams.get('q') || ''
      return postToTimbus('/Engine/Business/Search/action.ashx', 'act=searchfull&typ=2&key=' + encodeURIComponent(key))
    }
    if (action === 'info-bus') {
      const id = searchParams.get('id') || ''
      return postToTimbus('/Engine/Business/Search/action.ashx', 'act=fleetdetail&fid=' + id)
    }
    if (action === 'timing') {
      const state = searchParams.get('state') || ''
      return postToTimbus('/Engine/Business/Vehicle/action.ashx', 'act=partremained&State=true&StationID=' + state + '&FleetOver=')
    }
    if (action === 'route') {
      const lng = searchParams.get('lng') || ''
      const lat = searchParams.get('lat') || ''
      return postToTimbus('/Engine/Business/Route/action.ashx', 'act=geo2add&lng=' + lng + '&lat=' + lat)
    }
    return new Response(JSON.stringify({ error: 'Unknown timbus endpoint: ' + action }), {
      status: 404,
      headers: { ...CORS, 'Content-Type': 'application/json' },
    })
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
