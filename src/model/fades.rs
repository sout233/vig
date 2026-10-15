#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn independent_fades_and_old_project_conversion() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut p = Project::default();
        p.media.push(Media {
            path: file.path().into(),
            name: "视频".into(),
            duration: 3.,
            width: 640,
            height: 360,
            audio: false,
            frame_rate: None,
            generator: None,
        });
        p.append(0);
        let c = p.clip_mut(1).unwrap();
        c.fade_in = 1.;
        c.fade_out = 0.5;
        assert_eq!(c.brightness(0.25), 0.15625);
        assert_eq!(c.brightness(2.), 1.);
        assert_eq!(c.brightness(2.75), 0.5);
        p.validate().unwrap();
        let mut old = serde_json::to_value(&p).unwrap();
        old["version"] = 3.into();
        let clip = old["tracks"][0]["clips"][0].as_object_mut().unwrap();
        clip.remove("fade_in");
        clip.remove("fade_out");
        clip.insert("fade".into(), 0.75.into());
        let loaded = Project::from_json(&old.to_string()).unwrap();
        loaded.validate().unwrap();
        assert_eq!(loaded.version, 9);
        assert_eq!(loaded.clip(1).unwrap().fade_in, 0.75);
        assert_eq!(loaded.clip(1).unwrap().fade_out, 0.75);
    }
}
