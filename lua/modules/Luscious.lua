local dirurl = '/c/-/albums/frontpage/0/t/manga/sorted/new/page/'

function GetDirectoryPageNumber()
	if HTTP.GET(MODULE.RootURL .. dirurl .. '1/') then
		PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('//*[@class="pagination"]/*[@class="last"]/a/@href'):match('/(%d+)/*$')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. dirurl .. (URL + 1) .. '/') then
		CreateTXQuery(HTTP.Document).XPathHREFTitleAll('//*[@id="albums_wrapper"]//*[@class="item_cover"]/a', LINKS, NAMES)
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)

		MANGAINFO.CoverLink = MaybeFillHost(MODULE.RootURL, x.XPathString('//*[@class="album_cover_item"]//img/@src'))
		MANGAINFO.Title     = x.XPathString('//*[@class="album_cover"]/h2')
		MANGAINFO.Artists   = x.XPathString('//*[@id="tag_section"]/ol/li/a[starts-with(.,"Artist")]/text()[last()]')
		MANGAINFO.Genres    = x.XPathString('//*[@id="tag_section"]/ol/string-join(li/a/text()[last()],", ")')

		MANGAINFO.ChapterLinks.Add(URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		CreateTXQuery(HTTP.Document).XPathStringAll('//*[@class="picture_page"]//img/@data-src', TASK.PageLinks)
		return true
	else
		return false
	end
end

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'e0408ff713684f35923671af560054a5'
	m.Name                     = 'Luscious'
	m.RootURL                  = 'https://www.luscious.net'
	m.Category                 = 'H-Sites'
	m.SortedList               = true
	m.OnGetDirectoryPageNumber = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
end
