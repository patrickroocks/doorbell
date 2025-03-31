cmake_minimum_required(VERSION 3.16)

project(doorbell-client VERSION 0.1 LANGUAGES CXX)

set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(Qt6 REQUIRED COMPONENTS Core Widgets Mqtt)

set(PROJECT_SOURCES
		main.cpp
)

add_executable(doorbell-client
	${PROJECT_SOURCES}
	ringlistener.h ringlistener.cpp
	ringdialog.h ringdialog.cpp ringdialog.ui
	resources.qrc
	readme.md
	constants.h
	configstore.h configstore.cpp
	ringapp.h ringapp.cpp
	configdiagnosticsdialog.h configdiagnosticsdialog.cpp configdiagnosticsdialog.ui
	command.h command.cpp
	util.h util.cpp
	commandclient.h commandclient.cpp

)

target_link_libraries(doorbell-client PRIVATE Qt6::Widgets Qt6::Mqtt)

set_target_properties(doorbell-client PROPERTIES
	${BUNDLE_ID_OPTION}
	MACOSX_BUNDLE_BUNDLE_VERSION ${PROJECT_VERSION}
	MACOSX_BUNDLE_SHORT_VERSION_STRING ${PROJECT_VERSION_MAJOR}.${PROJECT_VERSION_MINOR}
	MACOSX_BUNDLE TRUE
	WIN32_EXECUTABLE TRUE
)

include(GNUInstallDirs)
install(TARGETS doorbell-client
	BUNDLE DESTINATION .
	LIBRARY DESTINATION ${CMAKE_INSTALL_LIBDIR}
	RUNTIME DESTINATION ${CMAKE_INSTALL_BINDIR}
)

if(QT_VERSION_MAJOR EQUAL 6)
	qt_finalize_executable(doorbell-client)
endif()
