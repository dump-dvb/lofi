use tlms::telegrams::r09::R09SaveTelegram;

/// Type alias for boxed iterator over [`R09SaveTelegram`]
pub type R09Iter = Box<dyn Iterator<Item = R09SaveTelegram>>;
