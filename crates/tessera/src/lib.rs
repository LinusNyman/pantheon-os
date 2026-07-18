//! Tessera — the tiles (§11.2). Small widgets that fold core JSON into a present
//! (a counter, a GPA, net worth) and draw into a caller-supplied ratatui `Buffer`
//! — a peer of Porticus, never a dependant (I5). A tile never stores its value;
//! it recomputes from the latest readings each frame (I1). Scaffold.
