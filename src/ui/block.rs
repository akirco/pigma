use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::{Block, Widget},
};

pub struct CornerBlock<'a> {
    block: Block<'a>,
    tl_color: Color,
    tr_color: Color,
    bl_color: Color,
    br_color: Color,
    h_size: u16,
    v_size: u16,
}

impl<'a> CornerBlock<'a> {
    pub fn new(block: Block<'a>) -> Self {
        Self {
            block,
            tl_color: Color::White,
            tr_color: Color::White,
            bl_color: Color::White,
            br_color: Color::White,
            h_size: 1,
            v_size: 1,
        }
    }

    pub fn corner_color(mut self, color: Color) -> Self {
        self.tl_color = color;
        self.tr_color = color;
        self.bl_color = color;
        self.br_color = color;
        self
    }

    pub fn corner_sizes(mut self, horizontal: u16, vertical: u16) -> Self {
        self.h_size = horizontal;
        self.v_size = vertical;
        self
    }

    pub fn block_padding(mut self, padding: ratatui::widgets::Padding) -> Self {
        self.block = self.block.padding(padding);
        self
    }

    pub fn inner(&self, area: Rect) -> Rect {
        self.block.inner(area)
    }
}

impl<'a> Widget for CornerBlock<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (tl, tr, bl, br) = (self.tl_color, self.tr_color, self.bl_color, self.br_color);

        self.block.render(area, buf);

        if area.width < 2 || area.height < 2 {
            return;
        }

        let top = area.top();
        let bottom = area.bottom() - 1;
        let left = area.left();
        let right = area.right() - 1;

        let max_h = self.h_size.min(area.width / 2);
        let max_v = self.v_size.min(area.height / 2);

        for i in 0..max_h {
            if let Some(cell) = buf.cell_mut((left + i, top)) {
                cell.fg = tl;
            }
            if let Some(cell) = buf.cell_mut((right - i, top)) {
                cell.fg = tr;
            }
            if let Some(cell) = buf.cell_mut((left + i, bottom)) {
                cell.fg = bl;
            }
            if let Some(cell) = buf.cell_mut((right - i, bottom)) {
                cell.fg = br;
            }
        }

        for i in 0..max_v {
            if let Some(cell) = buf.cell_mut((left, top + i)) {
                cell.fg = tl;
            }
            if let Some(cell) = buf.cell_mut((right, top + i)) {
                cell.fg = tr;
            }
            if let Some(cell) = buf.cell_mut((left, bottom - i)) {
                cell.fg = bl;
            }
            if let Some(cell) = buf.cell_mut((right, bottom - i)) {
                cell.fg = br;
            }
        }
    }
}
