//! Ship editor data model (spec §19). Exterior first (S17); interior (S18)
//! lands beside it. Pure data + pure composition functions — the Bevy editor
//! UI lives in the client; everything it saves or previews goes through the
//! contracts in these modules.

pub mod exterior;
