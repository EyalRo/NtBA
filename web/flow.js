// Which screen the page is on. Pure, so it can be tested without a browser.
//   title  - idle demo behind the Local / Online buttons
//   local  - the local two-pad game
//   online - the lobby and room-code panel
//   match  - an online match is being played
export function nextScreen(screen, event) {
  switch (`${screen}:${event}`) {
    case 'title:local': return 'local';
    case 'title:online': return 'online';
    case 'local:menu': return 'title';
    case 'online:menu': return 'title';
    case 'online:match-start': return 'match';
    case 'match:match-end': return 'online';
    default: return screen;
  }
}
