use clap::{App, Arg, SubCommand};
use fonttools::font::{self, Table};
use fonttools::glyf::{Glyph, Point};
use skia_safe::path::Verb;
use skia_safe::{simplify, Path};

fn draw_glyph(g: &mut Glyph) {
    if g.is_composite() || g.is_empty() {
        return;
    }
    let mut path = Path::default();
    g.insert_explicit_oncurves();
    for contour in g.contours.as_ref().unwrap() {
        path.move_to((contour[0].x as i32, contour[0].y as i32));
        let mut segment: Vec<&Point> = vec![];
        for pt in &contour[1..] {
            segment.push(pt);
            if pt.on_curve {
                match segment.len() {
                    1 => {
                        path.line_to((segment[0].x as i32, segment[0].y as i32));
                    }
                    2 => {
                        path.quad_to(
                            (segment[0].x as i32, segment[0].y as i32),
                            (segment[1].x as i32, segment[1].y as i32),
                        );
                    }
                    3 => {
                        path.cubic_to(
                            (segment[0].x as i32, segment[0].y as i32),
                            (segment[1].x as i32, segment[1].y as i32),
                            (segment[2].x as i32, segment[2].y as i32),
                        );
                    }
                    _ => {}
                };
                segment = vec![];
            }
        }
        if !segment.is_empty() {
            path.quad_to(
                (segment[0].x as i32, segment[0].y as i32),
                (contour[0].x as i32, contour[0].y as i32),
            );
            segment = vec![];
        }
        path.close();
    }
    if let Some(newpath) = simplify(&path) {
        g.contours = Some(skia_to_glyf(newpath));
    }
}

fn skia_to_glyf(p: Path) -> Vec<Vec<Point>> {
    let points_count = p.count_points();
    let mut points = vec![skia_safe::Point::default(); points_count];
    let _count_returned = p.get_points(&mut points);

    let verb_count = p.count_verbs();
    let mut verbs = vec![0_u8; verb_count];
    let _count_returned_verbs = p.get_verbs(&mut verbs);
    let mut new_contour: Vec<Point> = vec![];
    let mut new_glyph: Vec<Vec<Point>> = vec![];
    let mut cur_pt = 0;
    for verb in verbs {
        if verb > 4 {
            new_glyph.push(new_contour);
            new_contour = vec![];
            continue;
        }
        if verb < 2 {
            new_contour.push(Point {
                x: points[cur_pt].x as i16,
                y: points[cur_pt].y as i16,
                on_curve: true,
            });
            cur_pt += 1;
        } else {
            new_contour.push(Point {
                x: points[cur_pt].x as i16,
                y: points[cur_pt].y as i16,
                on_curve: false,
            });
            cur_pt += 1;
            new_contour.push(Point {
                x: points[cur_pt].x as i16,
                y: points[cur_pt].y as i16,
                on_curve: true,
            });
            cur_pt += 1;
        }
    }
    new_glyph
}

use std::fs::File;
use std::io;

fn main() {
    let matches = App::new("ttf-remove-overlap")
        .about("Removes overlap from TTF files")
        .arg(
            Arg::with_name("INPUT")
                .help("Sets the input file to use")
                .required(false),
        )
        .arg(
            Arg::with_name("OUTPUT")
                .help("Sets the output file to use")
                .required(false),
        )
        .get_matches();
    let mut infont = if matches.is_present("INPUT") {
        let filename = matches.value_of("INPUT").unwrap();
        let infile = File::open(filename).unwrap();
        font::load(infile)
    } else {
        font::load(io::stdin())
    }
    .expect("Could not parse font");
    let names = infont
        .get_table(b"post")
        .unwrap()
        .unwrap()
        .post_unchecked()
        .glyphnames
        .as_ref()
        .unwrap()
        .clone();
    if let Table::Glyf(glyf) = infont.get_table(b"glyf").unwrap().unwrap() {
        for (i, glyph) in glyf.glyphs.iter_mut().enumerate() {
            if let Some(glyph) = glyph {
                draw_glyph(glyph);
            }
        }
    }

    if matches.is_present("OUTPUT") {
        let mut outfile = File::create(matches.value_of("OUTPUT").unwrap())
            .expect("Could not open file for writing");
        infont.save(&mut outfile);
    } else {
        infont.save(&mut io::stdout());
    };
}