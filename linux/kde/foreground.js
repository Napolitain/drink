// The loader supplies drinkPid. Keep window references inside KWin.
let previous = null;
let overlayFocused = false;
function report(window) {
    if (window) {
        overlayFocused = window.pid === drinkPid;
        if (!overlayFocused)
            previous = window;
    }
    callDBus("io.github.napolitain.Drink", "/Foreground",
             "io.github.napolitain.Drink.Foreground", "report",
             window ? String(window.desktopFileName) : "",
             window ? String(window.resourceClass) : "", window ? window.pid : 0);
}
workspace.windowActivated.connect(report);
workspace.windowRemoved.connect(window => {
    if (previous === window)
        previous = null;
    if (window.pid !== drinkPid || !overlayFocused || !previous)
        return;
    const remaining = workspace.windowList().some(w => w !== window && w.pid === drinkPid);
    if (!remaining && (!workspace.activeWindow || workspace.activeWindow.pid === drinkPid)) {
        overlayFocused = false;
        const onDesktop = !previous.desktops.length || previous.desktops.includes(workspace.currentDesktop);
        const onActivity = !previous.activities.length || previous.activities.includes(workspace.currentActivity);
        if (!previous.minimized && onDesktop && onActivity)
            workspace.activeWindow = previous;
    }
});
report(workspace.activeWindow);
