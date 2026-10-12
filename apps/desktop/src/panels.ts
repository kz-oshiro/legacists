import type { DockviewApi, AddPanelOptions } from 'dockview-vue';

export const panelDefinitions = [
  { id: 'explorer', component: 'explorer', title: 'Explorer', initialWidth: 280, position: { referencePanel: 'graph', direction: 'left' } },
  { id: 'graph', component: 'graph', title: '関連グラフ', position: { referencePanel: 'explorer', direction: 'right' } },
  { id: 'overview', component: 'overview', title: '概要グラフ', position: { referencePanel: 'graph', direction: 'within' } },
  { id: 'source', component: 'source', title: 'Source / 根拠', initialWidth: 400, position: { referencePanel: 'graph', direction: 'right' } },
  { id: 'export', component: 'export', title: 'AI Export', initialHeight: 250, position: { referencePanel: 'graph', direction: 'below' } },
  { id: 'diagnostics', component: 'diagnostics', title: 'Diagnostics', position: { referencePanel: 'export', direction: 'within' } },
] as const satisfies readonly AddPanelOptions[];

export type PanelId = typeof panelDefinitions[number]['id'];

export function showPanel(api: DockviewApi, id: PanelId) {
  let panel = api.getPanel(id);
  if (!panel) {
    const definition = panelDefinitions.find(item => item.id === id)!;
    const preferred = api.getPanel(definition.position.referencePanel);
    const fallback = api.activePanel ?? api.panels[0];
    // A missing reference must not prevent recovery or move existing panels.
    const position: AddPanelOptions['position'] = preferred
      ? definition.position
      : fallback ? { referencePanel: fallback.id, direction: 'within' } : undefined;
    panel = api.addPanel({ ...definition, position,
      ...(id === 'source' ? { initialWidth: Math.min(400, api.width * .28) } : {}),
    });
  }
  panel.api.setActive();
}

export function showAllPanels(api: DockviewApi) {
  const empty = api.totalPanels === 0;
  for (const definition of panelDefinitions) {
    if (!api.getPanel(definition.id)) showPanel(api, definition.id);
  }
  // Only an empty workspace needs initial sizing; preserve any existing layout.
  if (empty) {
    api.getPanel('explorer')?.api.setSize({ width: 280 });
    api.getPanel('source')?.api.setSize({ width: Math.min(400, api.width * .28) });
    showPanel(api, 'graph');
    showPanel(api, 'export');
  }
}
