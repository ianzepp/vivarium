use super::default_socket_path;
use tempfile::tempdir;
use vivarium::mailspace::Mailspace;

#[test]
fn default_socket_belongs_to_selected_mailspace() {
    let project = tempdir().unwrap();
    let mailspace = Mailspace::init(Some(project.path())).unwrap();

    let socket = default_socket_path(Some(project.path())).unwrap();

    assert_eq!(socket, mailspace.dir.join("vivi-pty.sock"));
}
