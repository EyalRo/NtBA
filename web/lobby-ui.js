// The player list. Names come from other players, so they are only ever set with textContent.

const LINK_TEXT = {
  self: 'you',
  open: 'connected',
  connecting: 'connecting…',
  unreachable: "can't reach for chat or challenges",
};

/**
 * @param {HTMLElement} list a <ul>
 * @param {{id: string, name: string, status: string, link: string, muted: boolean}[]} players
 * @param {{onMute?: (id: string, on: boolean) => void}} actions
 */
export function renderRoster(list, players, actions = {}) {
  const items = players.map((p) => {
    const li = document.createElement('li');
    li.className = `player player-${p.link}`;
    const name = document.createElement('span');
    name.className = 'player-name';
    name.dir = 'auto';
    name.textContent = p.name;
    const info = document.createElement('span');
    info.className = 'player-info';
    info.textContent = [p.status === 'in-match' ? 'in a match' : '', LINK_TEXT[p.link] ?? ''].filter(Boolean).join(' · ');
    li.append(name, info);
    if (p.link !== 'self') {
      const mute = document.createElement('button');
      mute.type = 'button';
      mute.textContent = p.muted ? 'Unmute' : 'Mute';
      mute.addEventListener('click', () => actions.onMute?.(p.id, !p.muted));
      li.append(mute);
    }
    return li;
  });
  list.replaceChildren(...items);
}
