// Runs one connect attempt on `link`. If it fails after the page has moved on to another link
// (`getCurrent()` no longer returns this one), the stale link is closed quietly and nothing else is
// touched; otherwise `onFailure(err)` handles the error. Pure of the DOM so it can be tested.
// Resolves true when the attempt succeeded.
export async function guardedAttempt(link, getCurrent, attempt, onFailure) {
  try {
    await attempt(link);
    return true;
  } catch (err) {
    if (getCurrent() !== link) {
      link?.close();
      return false;
    }
    onFailure(err);
    return false;
  }
}
