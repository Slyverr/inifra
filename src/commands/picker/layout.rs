pub(super) const PREVIEW_SCROLL_ROWS: usize = 8;
pub(super) const MAX_TERMINAL_ROWS: usize = 32;
pub(super) const TERMINAL_ROW_RESERVE: u16 = 6;
const SIDE_BY_SIDE_MIN_WIDTH: usize = 100;
const SIDE_BY_SIDE_MIN_HEIGHT: usize = 18;
const LIST_WIDTH_PERCENT: usize = 42;
const LIST_MIN_WIDTH: usize = 36;
const DETAIL_MIN_WIDTH: usize = 48;
const FOOTER_GAP_MIN_HEIGHT: usize = 16;
const STACKED_DETAIL_RESERVE: usize = 9;
const STACKED_HEIGHT_RESERVE: usize = 7;
const STACKED_LIST_MIN_ROWS: usize = 4;
const STACKED_LIST_MAX_ROWS: usize = 14;

pub(super) struct PickerLayout {
    pub(super) top: usize,
    pub(super) height: usize,
    pub(super) footer_y: usize,
    pub(super) list_x: usize,
    pub(super) list_width: usize,
    pub(super) list_top: usize,
    pub(super) list_rows: usize,
    pub(super) detail_x: usize,
    pub(super) detail_width: usize,
    pub(super) detail_top: usize,
    pub(super) detail_rows: usize,
}

impl PickerLayout {
    pub(super) fn calculate(
        width: usize,
        terminal_height: usize,
        session_top: u16,
        session_rows: usize,
        entry_count: usize,
    ) -> Self {
        let top = usize::from(session_top).min(terminal_height.saturating_sub(1));
        let height = session_rows.min(terminal_height.saturating_sub(top)).max(1);
        let footer_y = top + height - 1;
        let footer_gap = usize::from(height >= FOOTER_GAP_MIN_HEIGHT);
        let content_bottom = footer_y.saturating_sub(footer_gap);
        let content_top = top + 2;
        let side_by_side = width >= SIDE_BY_SIDE_MIN_WIDTH && height >= SIDE_BY_SIDE_MIN_HEIGHT;

        if side_by_side {
            let list_width = (width * LIST_WIDTH_PERCENT / 100)
                .max(LIST_MIN_WIDTH)
                .min(width.saturating_sub(DETAIL_MIN_WIDTH));
            let detail_x = list_width + 4;
            let list_top = content_top;

            Self {
                top,
                height,
                footer_y,
                list_x: 0,
                list_width,
                list_top,
                list_rows: content_bottom
                    .saturating_sub(list_top)
                    .min(entry_count)
                    .max(1),
                detail_x,
                detail_width: width.saturating_sub(detail_x),
                detail_top: content_top,
                detail_rows: content_bottom.saturating_sub(content_top),
            }
        } else {
            let list_top = content_top;
            let max_list_rows = content_bottom
                .saturating_sub(list_top + STACKED_DETAIL_RESERVE)
                .max(1);
            let list_rows = ((height.saturating_sub(STACKED_HEIGHT_RESERVE) / 2)
                .clamp(STACKED_LIST_MIN_ROWS, STACKED_LIST_MAX_ROWS))
            .min(max_list_rows)
            .min(entry_count.max(1));
            let detail_top = list_top + list_rows + 1;

            Self {
                top,
                height,
                footer_y,
                list_x: 0,
                list_width: width,
                list_top,
                list_rows,
                detail_x: 0,
                detail_width: width,
                detail_top,
                detail_rows: content_bottom.saturating_sub(detail_top),
            }
        }
    }
}
