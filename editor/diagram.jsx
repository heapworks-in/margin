// Excalidraw in a full-screen modal. Bundled into static/vendor/diagram.js (loaded on first use).
// window.Diagram.open(sceneJSON|null) -> Promise<{svg, scene}|null>
import React, { useRef } from 'react'
import { createRoot } from 'react-dom/client'
import { Excalidraw, exportToSvg, serializeAsJSON, convertToExcalidrawElements } from '@excalidraw/excalidraw'

window.EXCALIDRAW_ASSET_PATH = window.EXCALIDRAW_ASSET_PATH || new URL('vendor/excalidraw/', location.href).href

function Modal({ scene, onDone, onCancel }) {
  const api = useRef(null)
  const done = async () => {
    const els = api.current.getSceneElements(), state = api.current.getAppState(), files = api.current.getFiles()
    if (!els.length) return onCancel()
    const svg = await exportToSvg({ elements: els, appState: { ...state, exportBackground: false, exportWithDarkMode: true, exportEmbedScene: false }, files })
    onDone({ svg: svg.outerHTML, scene: serializeAsJSON(els, state, files, 'local') })
  }
  const initial = scene ? JSON.parse(scene) : null
  return (
    <div className="dg-modal">
      <div className="dg-bar">
        <b>Diagram</b>
        <span className="dg-hint">Draw, then insert. You can reopen and edit it later by clicking the diagram.</span>
        <button type="button" className="dg-btn ghost" onClick={onCancel}>Cancel</button>
        <button type="button" className="dg-btn" onClick={done}>Insert into document</button>
      </div>
      <div className="dg-canvas">
        <Excalidraw
          excalidrawAPI={a => { api.current = a; window.Diagram._api = a }}
          theme="dark"
          initialData={{ elements: initial ? initial.elements : [], appState: initial ? { ...initial.appState, collaborators: new Map() } : {}, files: initial ? initial.files : {}, libraryItems: window.Diagram.library || [] }}
          onLibraryChange={items => { window.Diagram.library = items; window.Diagram.onLibrary && window.Diagram.onLibrary(items) }}
          UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false, export: false, saveAsImage: false } }}
        />
      </div>
    </div>
  )
}
window.Diagram = {
  _api: null,
  library: [],
  onLibrary: null,
  /** SVG + scene for elements built with Diagram.elements(), without opening the editor. */
  async render(elements) {
    const svg = await exportToSvg({ elements, appState: { exportBackground: false, exportWithDarkMode: true, exportEmbedScene: false, viewBackgroundColor: '#0E1117' }, files: {} })
    return { svg: svg.outerHTML, scene: serializeAsJSON(elements, {}, {}, 'local') }
  },
  elements: convertToExcalidrawElements,
  open(scene) {
    return new Promise(resolve => {
      const host = document.createElement('div'); document.body.append(host)
      const root = createRoot(host)
      const close = v => { root.unmount(); host.remove(); resolve(v) }
      root.render(<Modal scene={scene} onDone={close} onCancel={() => close(null)} />)
    })
  },
}
