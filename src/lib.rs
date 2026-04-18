//! Core skills: umbrella crate that links to skill crates and re-exports their API.
//!
//! Consumers (e.g. desktop-runner) depend only on `core-skills` to use enabled skill groups.
//! The backend can import `ENABLED_SKILL_IDS` for classifier prompt construction.

#[cfg(all(not(clippy), feature = "macos", feature = "windows"))]
compile_error!("features `macos` and `windows` cannot be enabled together");
#[cfg(all(not(clippy), feature = "macos", feature = "ios"))]
compile_error!("features `macos` and `ios` cannot be enabled together");
#[cfg(all(not(clippy), feature = "windows", feature = "ios"))]
compile_error!("features `windows` and `ios` cannot be enabled together");

/// Skill IDs enabled for the current feature selection.
pub static ENABLED_SKILL_IDS: &[&str] = &[
    #[cfg(feature = "core-common")]
    "skill_weather",
    #[cfg(feature = "core-common")]
    "skill_time",
    #[cfg(feature = "core-common")]
    "skill_distance",
    #[cfg(feature = "core-common")]
    "skill_sports_live",
    #[cfg(feature = "core-common")]
    "skill_holiday_lookup",
    #[cfg(feature = "core-common")]
    "skill_fuel_price_lookup",
    #[cfg(feature = "core-common")]
    "skill_horoscope_daily",
    #[cfg(feature = "core-common")]
    "skill_news_headlines",
    #[cfg(feature = "core-common")]
    "skill_calculator",
    #[cfg(feature = "core-common")]
    "skill_unit_conversion",
    #[cfg(feature = "core-common")]
    "skill_currency",
    #[cfg(feature = "core-common")]
    "skill_air_quality",
    #[cfg(feature = "core-common")]
    "skill_dictionary",
    #[cfg(feature = "core-common")]
    "skill_translate",
    #[cfg(feature = "core-common")]
    "skill_spotify",
    #[cfg(feature = "core-common")]
    "skill_calendar",
    #[cfg(feature = "core-common")]
    "skill_meeting_notes",
    #[cfg(feature = "core-common")]
    "skill_email",
    #[cfg(feature = "core-common")]
    "skill_briefing",
    #[cfg(feature = "core-common")]
    "skill_journal",
    #[cfg(feature = "core-common")]
    "skill_screen_ocr",
    #[cfg(feature = "macos")]
    "skill_smart_home",
    #[cfg(feature = "core-common")]
    "skill_media",
    #[cfg(feature = "macos")]
    "skill_computer",
    #[cfg(feature = "macos")]
    "skill_record_window",
    #[cfg(feature = "macos")]
    "skill_screenshot",
    #[cfg(feature = "macos")]
    "skill_app_switcher",
    #[cfg(feature = "macos")]
    "skill_reminder",
    #[cfg(feature = "macos")]
    "skill_timer",
    #[cfg(feature = "macos")]
    "skill_shopping_list",
    #[cfg(feature = "macos")]
    "skill_message",
    #[cfg(feature = "macos")]
    "skill_volume",
    #[cfg(feature = "windows")]
    "skill_windows_apple_music",
];

/// Backward-compatible alias for enabled skills in the active build.
pub static ALL_SKILL_IDS: &[&str] = ENABLED_SKILL_IDS;

#[cfg(feature = "core-common")]
pub use skill_air_quality::{
    AirQualityError, AirQualityLocation, AirQualityProvider, AirQualityResult, AirQualitySkill,
    AqiCategory, HttpAirQualitySkill, MockAirQualitySkill, OpenMeteoAirQualityProvider,
};
#[cfg(feature = "macos")]
pub use skill_app_switcher::{
    AppSwitcherResult, AppSwitcherSkill, AppSwitcherSkillError, MacOsAppSwitcherSkill,
    MockAppSwitcherSkill,
};
#[cfg(feature = "core-common")]
pub use skill_briefing::{
    BriefingError, BriefingQuery, BriefingResult, BriefingSection, BriefingSkill,
    ComposedBriefingSkill,
};
#[cfg(feature = "core-common")]
pub use skill_calculator::{
    CalculatorResult, CalculatorSkill, CalculatorSkillError, LocalCalculatorSkill,
    MockCalculatorSkill,
};
#[cfg(feature = "core-common")]
pub use skill_calendar::{
    AppleCalendarSkill, CalendarAction, CalendarError, CalendarEvent, CalendarQuery,
    CalendarResult, CalendarSkill, GoogleCalendarAuth, GoogleCalendarRequest,
    GoogleCalendarResponse, GoogleCalendarSkill, GoogleCalendarTransport,
    HttpGoogleCalendarTransport, MockCalendarSkill, StaticGoogleCalendarAuth,
};
#[cfg(feature = "macos")]
pub use skill_computer::{
    ComputerResult, ComputerSkill, ComputerSkillError, MacOsComputerSkill, MockComputerSkill,
};
#[cfg(feature = "core-common")]
pub use skill_currency::{
    CurrencyError, CurrencyQuery, CurrencyRateProvider, CurrencyResult, CurrencySkill,
    FrankfurterProvider, HttpCurrencySkill, MockCurrencySkill,
};
#[cfg(feature = "core-common")]
pub use skill_dictionary::{
    DictionaryApiDevProvider, DictionaryEntry, DictionaryError, DictionaryProvider,
    DictionaryResult, DictionarySkill, HttpDictionarySkill, MockDictionarySkill,
};
#[cfg(feature = "core-common")]
pub use skill_distance::{
    DistanceResult, DistanceSkill, DistanceSkillError, MockDistanceSkill, OpenMeteoDistanceSkill,
};
#[cfg(feature = "core-common")]
pub use skill_email::{
    AppleMailEmailSkill, EmailAction, EmailError, EmailLlm, EmailMessage, EmailQuery, EmailResult,
    EmailSkill, ImapEmailSkill, ImapFetcher, MockEmailSkill, TriageBucket,
};
#[cfg(feature = "core-common")]
pub use skill_fuel_price_lookup::{
    FuelPriceLookupError, FuelPriceLookupQuery, FuelPriceLookupResult, FuelPriceLookupSkill,
    HttpFuelPriceLookupSkill, MockFuelPriceLookupSkill,
};
#[cfg(feature = "core-common")]
pub use skill_holiday_lookup::{
    HolidayLookupError, HolidayLookupResult, HolidayLookupSkill, HolidayMatch, HolidayQuery,
    HttpHolidayLookupSkill, MockHolidayLookupSkill,
};
#[cfg(feature = "core-common")]
pub use skill_horoscope_daily::{
    HoroscopeDailyError, HoroscopeDailyQuery, HoroscopeDailyResult, HoroscopeDailySkill,
    HttpHoroscopeDailySkill, MockHoroscopeDailySkill,
};
#[cfg(feature = "core-common")]
pub use skill_journal::{
    JournalAction, JournalEntry, JournalError, JournalResult, JournalSkill, JournalStats,
    LocalJournalSkill, MockJournalSkill, Sentiment, SqliteJournalStore,
};
#[cfg(feature = "core-common")]
pub use skill_media::{MacOsMusicSkill, MediaResult, MediaSkill, MediaSkillError, MockMediaSkill};
#[cfg(feature = "core-common")]
pub use skill_meeting_notes::{
    ActionItem, LlmMeetingNotesSkill, MeetingNotesError, MeetingNotesLlm, MeetingNotesQuery,
    MeetingNotesResult, MeetingNotesSkill, MockMeetingNotesSkill,
};
#[cfg(feature = "macos")]
pub use skill_message::{
    MacOsMessagesSkill, MessageResult, MessageSkill, MessageSkillError, MockMessageSkill,
};
#[cfg(feature = "core-common")]
pub use skill_news_headlines::{
    collect_summaries as collect_news_summaries, stream_summaries as stream_news_summaries,
    NewsSummaryEvent, NewsSummaryLlm, SummarizedHeadline,
};
#[cfg(feature = "core-common")]
pub use skill_news_headlines::{
    HttpNewsHeadlinesSkill, MockNewsHeadlinesSkill, NewsHeadline, NewsHeadlinesError,
    NewsHeadlinesQuery, NewsHeadlinesResult, NewsHeadlinesSkill,
};
#[cfg(feature = "macos")]
pub use skill_record_window::{
    MacOsRecordWindowSkill, MockRecordWindowSkill, RecordWindowResult, RecordWindowSkill,
    RecordWindowSkillError,
};
#[cfg(feature = "macos")]
pub use skill_reminder::{
    MacOsReminderSkill, MockReminderSkill, ReminderResult, ReminderSkill, ReminderSkillError,
};
#[cfg(feature = "core-common")]
pub use skill_screen_ocr::{
    ComposedScreenOcrSkill, MockScreenOcrSkill, OcrEngine, ScreenOcrError, ScreenOcrLlm,
    ScreenOcrQuery, ScreenOcrResult, ScreenOcrSkill,
};
#[cfg(feature = "macos")]
pub use skill_screenshot::{
    MacOsScreenshotSkill, MockScreenshotSkill, ScreenshotResult, ScreenshotSkill,
    ScreenshotSkillError,
};
#[cfg(feature = "macos")]
pub use skill_shopping_list::{
    MacOsNotesShoppingListSkill, MockShoppingListSkill, ShoppingListResult, ShoppingListSkill,
    ShoppingListSkillError,
};
#[cfg(feature = "macos")]
pub use skill_smart_home::{
    DeviceState, HueSmartHomeSkill, MockSmartHomeSkill, SmartHomeResult, SmartHomeSkill,
    SmartHomeSkillError,
};
#[cfg(feature = "core-common")]
pub use skill_sports_live::{
    HttpSportsLiveSkill, MockSportsLiveSkill, SportsEvent, SportsLiveError, SportsLiveQuery,
    SportsLiveResult, SportsLiveSkill,
};
#[cfg(feature = "core-common")]
pub use skill_spotify::{
    HttpSpotifyTransport, SpotifyAuth, SpotifyError, SpotifyHttpResponse, SpotifyPlayState,
    SpotifyRequest, SpotifySkill, SpotifyTrack, SpotifyTransport, StaticSpotifyAuth,
};
#[cfg(feature = "core-common")]
pub use skill_time::{MockTimeSkill, OpenMeteoTimeSkill, TimeResult, TimeSkill, TimeSkillError};
#[cfg(feature = "macos")]
pub use skill_timer::{
    MacOsClockTimerSkill, MockTimerSkill, TimerRegistry, TimerResult, TimerSkill, TimerSkillError,
    TimerStatus,
};
#[cfg(feature = "core-common")]
pub use skill_translate::{
    LlmTranslateSkill, MockTranslateSkill, TranslateError, TranslateQuery, TranslateResult,
    TranslateSkill, TranslationLlm,
};
#[cfg(feature = "core-common")]
pub use skill_unit_conversion::{
    ConversionResult, Dimension, LocalUnitConversionSkill, MockUnitConversionSkill,
    UnitConversionError, UnitConversionSkill,
};
#[cfg(feature = "macos")]
pub use skill_volume::{
    MacOsVolumeSkill, MockVolumeSkill, VolumeResult, VolumeSkill, VolumeSkillError,
};
#[cfg(feature = "core-common")]
pub use skill_weather::{
    AlertSeverity, DailyForecast, MockWeatherSkill, OpenMeteoWeatherSkill, ResolvedLocation,
    SevereAlert, WeatherResult, WeatherSkill, WeatherSkillError,
};
#[cfg(feature = "windows")]
pub use skill_windows_apple_music::{
    MockWindowsAppleMusicSkill, WindowsAppleMusic, WindowsAppleMusicResult, WindowsAppleMusicSkill,
    WindowsAppleMusicSkillError,
};
